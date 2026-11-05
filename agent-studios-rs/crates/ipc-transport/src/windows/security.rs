use crate::error::TransportError;
use std::alloc::Layout;
use std::ffi::c_void;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::System::Threading::*;

const SECURITY_DESCRIPTOR_REVISION: u32 = 1;
const ACCESS_ALLOWED_ACE_TYPE_U8: u8 = 0;

struct HandleGuard(HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != -1isize as HANDLE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

/// RAII wrapper for Windows user-only Security Attributes.
/// Strictly restricts access exclusively to the current user SID with GENERIC_ALL.
/// Fails closed on any error (NULL DACL is strictly forbidden).
pub struct UserOnlySecurityAttributes {
    _token_user_buf: Vec<u8>,
    acl_ptr: *mut ACL,
    acl_layout: Layout,
    _sd: Box<SECURITY_DESCRIPTOR>,
    sa: Box<SECURITY_ATTRIBUTES>,
}

unsafe impl Send for UserOnlySecurityAttributes {}
unsafe impl Sync for UserOnlySecurityAttributes {}

impl UserOnlySecurityAttributes {
    /// Constructs a new user-only security descriptor.
    /// Fails closed if any step fails.
    pub fn new() -> Result<Self, TransportError> {
        let process_handle = unsafe { GetCurrentProcess() };
        let mut raw_token_handle: HANDLE = std::ptr::null_mut();

        let open_res =
            unsafe { OpenProcessToken(process_handle, TOKEN_QUERY, &mut raw_token_handle) };
        if open_res == 0 || raw_token_handle.is_null() {
            return Err(TransportError::SecurityDescriptor(format!(
                "OpenProcessToken failed with code {}",
                unsafe { GetLastError() }
            )));
        }
        let _token_guard = HandleGuard(raw_token_handle);

        let mut return_length: u32 = 0;
        unsafe {
            GetTokenInformation(
                raw_token_handle,
                TokenUser,
                std::ptr::null_mut(),
                0,
                &mut return_length,
            )
        };

        let last_err = unsafe { GetLastError() };
        if last_err != ERROR_INSUFFICIENT_BUFFER {
            return Err(TransportError::SecurityDescriptor(format!(
                "GetTokenInformation query failed with code {}",
                last_err
            )));
        }
        if return_length == 0 {
            return Err(TransportError::SecurityDescriptor(
                "GetTokenInformation returned 0 required length".into(),
            ));
        }

        let mut token_user_buf = vec![0u8; return_length as usize];
        let info_res = unsafe {
            GetTokenInformation(
                raw_token_handle,
                TokenUser,
                token_user_buf.as_mut_ptr() as *mut c_void,
                return_length,
                &mut return_length,
            )
        };
        if info_res == 0 {
            return Err(TransportError::SecurityDescriptor(format!(
                "GetTokenInformation failed with code {}",
                unsafe { GetLastError() }
            )));
        }

        let token_user = token_user_buf.as_ptr() as *const TOKEN_USER;
        let sid = unsafe { (*token_user).User.Sid };
        if sid.is_null() {
            return Err(TransportError::SecurityDescriptor(
                "TokenUser returned NULL SID".into(),
            ));
        }
        if unsafe { IsValidSid(sid) } == 0 {
            return Err(TransportError::SecurityDescriptor(
                "TokenUser returned invalid SID".into(),
            ));
        }

        let sid_len = unsafe { GetLengthSid(sid) };
        if sid_len == 0 {
            return Err(TransportError::SecurityDescriptor(
                "GetLengthSid returned 0".into(),
            ));
        }

        // Calculate DACL size: sizeof(ACL) + sizeof(ACCESS_ALLOWED_ACE) - sizeof(u32) + sid_len
        let ace_size = std::mem::size_of::<ACCESS_ALLOWED_ACE>()
            .checked_sub(std::mem::size_of::<u32>())
            .and_then(|base| base.checked_add(sid_len as usize))
            .ok_or_else(|| {
                TransportError::SecurityDescriptor("ACE size calculation overflow".into())
            })?;

        let acl_size = std::mem::size_of::<ACL>()
            .checked_add(ace_size)
            .ok_or_else(|| {
                TransportError::SecurityDescriptor("ACL size calculation overflow".into())
            })?;

        // 8-byte align the allocation
        let acl_size_aligned = (acl_size + 7) & !7;
        let acl_layout = Layout::from_size_align(acl_size_aligned, 8).map_err(|e| {
            TransportError::SecurityDescriptor(format!("Invalid ACL layout: {}", e))
        })?;

        let acl_ptr = unsafe { std::alloc::alloc_zeroed(acl_layout) as *mut ACL };
        if acl_ptr.is_null() {
            return Err(TransportError::SecurityDescriptor(
                "Failed to allocate ACL memory".into(),
            ));
        }

        if unsafe { InitializeAcl(acl_ptr, acl_size_aligned as u32, ACL_REVISION) } == 0 {
            let err = unsafe { GetLastError() };
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(format!(
                "InitializeAcl failed with code {}",
                err
            )));
        }

        if unsafe { AddAccessAllowedAce(acl_ptr, ACL_REVISION, GENERIC_ALL, sid) } == 0 {
            let err = unsafe { GetLastError() };
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(format!(
                "AddAccessAllowedAce failed with code {}",
                err
            )));
        }

        if unsafe { IsValidAcl(acl_ptr) } == 0 {
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(
                "Created ACL is invalid".into(),
            ));
        }

        // Verify ACL contains exactly one ACCESS_ALLOWED_ACE for our SID
        if unsafe { (*acl_ptr).AceCount } != 1 {
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(
                "ACL does not contain exactly one ACE".into(),
            ));
        }

        let mut ace_raw: *mut c_void = std::ptr::null_mut();
        if unsafe { GetAce(acl_ptr, 0, &mut ace_raw) } == 0 || ace_raw.is_null() {
            let err = unsafe { GetLastError() };
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(format!(
                "GetAce failed with code {}",
                err
            )));
        }

        let ace = unsafe { &*(ace_raw as *const ACCESS_ALLOWED_ACE) };
        if ace.Header.AceType != ACCESS_ALLOWED_ACE_TYPE_U8 {
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(
                "ACE is not of type ACCESS_ALLOWED_ACE".into(),
            ));
        }
        if ace.Mask != GENERIC_ALL {
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(
                "ACE mask is not GENERIC_ALL".into(),
            ));
        }
        let ace_sid = &ace.SidStart as *const u32 as PSID;
        if unsafe { EqualSid(sid, ace_sid) } == 0 {
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(
                "ACE SID does not match token SID".into(),
            ));
        }

        let mut sd = Box::new(unsafe { std::mem::zeroed::<SECURITY_DESCRIPTOR>() });
        if unsafe {
            InitializeSecurityDescriptor(
                sd.as_mut() as *mut _ as *mut c_void,
                SECURITY_DESCRIPTOR_REVISION,
            )
        } == 0
        {
            let err = unsafe { GetLastError() };
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(format!(
                "InitializeSecurityDescriptor failed with code {}",
                err
            )));
        }

        // Set the explicit DACL on security descriptor (NULL DACL is strictly forbidden)
        if unsafe { SetSecurityDescriptorDacl(sd.as_mut() as *mut _ as *mut c_void, 1, acl_ptr, 0) }
            == 0
        {
            let err = unsafe { GetLastError() };
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(format!(
                "SetSecurityDescriptorDacl failed with code {}",
                err
            )));
        }

        // Verify security descriptor is valid and DACL is present and non-null
        if unsafe { IsValidSecurityDescriptor(sd.as_mut() as *mut _ as *mut c_void) } == 0 {
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(
                "Final Security Descriptor is invalid".into(),
            ));
        }

        let mut dacl_present: BOOL = 0;
        let mut dacl_ptr_check: *mut ACL = std::ptr::null_mut();
        let mut dacl_defaulted: BOOL = 0;
        if unsafe {
            GetSecurityDescriptorDacl(
                sd.as_mut() as *mut _ as *mut c_void,
                &mut dacl_present,
                &mut dacl_ptr_check,
                &mut dacl_defaulted,
            )
        } == 0
        {
            let err = unsafe { GetLastError() };
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(format!(
                "GetSecurityDescriptorDacl verification failed with code {}",
                err
            )));
        }

        if dacl_present == 0 || dacl_ptr_check.is_null() {
            unsafe { std::alloc::dealloc(acl_ptr as *mut u8, acl_layout) };
            return Err(TransportError::SecurityDescriptor(
                "Security Descriptor has NULL or absent DACL".into(),
            ));
        }

        let sa = Box::new(SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.as_mut() as *mut _ as *mut c_void,
            bInheritHandle: 0,
        });

        Ok(Self {
            _token_user_buf: token_user_buf,
            acl_ptr,
            acl_layout,
            _sd: sd,
            sa,
        })
    }

    /// Exposes a raw pointer to SECURITY_ATTRIBUTES suitable for Windows APIs.
    pub fn as_security_attributes_ptr(&self) -> *const c_void {
        self.sa.as_ref() as *const _ as *const c_void
    }

    /// Exposes a mutable raw pointer to SECURITY_ATTRIBUTES suitable for Windows APIs.
    pub fn as_security_attributes_mut_ptr(&self) -> *mut c_void {
        self.sa.as_ref() as *const _ as *mut c_void
    }

    /// Exposes raw pointer to the underlying SECURITY_DESCRIPTOR.
    pub fn as_security_descriptor_ptr(&self) -> *const c_void {
        self.sa.lpSecurityDescriptor as *const c_void
    }

    /// Validates that the underlying security descriptor is valid.
    pub fn is_valid(&self) -> bool {
        let sd_ptr = self.sa.lpSecurityDescriptor;
        if sd_ptr.is_null() {
            return false;
        }
        unsafe { IsValidSecurityDescriptor(sd_ptr) != 0 }
    }
}

impl Drop for UserOnlySecurityAttributes {
    fn drop(&mut self) {
        if !self.acl_ptr.is_null() {
            unsafe {
                std::alloc::dealloc(self.acl_ptr as *mut u8, self.acl_layout);
                self.acl_ptr = std::ptr::null_mut();
            }
        }
    }
}
