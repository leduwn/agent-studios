; Agent Studios Windows x64 NSIS Installer Script (Unsigned RC2 Blueprint)
; Targets Windows 11 x64 with bundled agsd.exe, ags.exe, AgentStudios.exe, Code-OSS extension (.vsix), and canonical paths.

!define APP_NAME "Agent Studios"
!define APP_VERSION "1.0.0-rc2"
!define PUBLISHER "Agent Studios Contributors"
!define INSTALL_DIR "$LOCALAPPDATA\Programs\AgentStudios"

Name "${APP_NAME} ${APP_VERSION}"
OutFile "AgentStudios-${APP_VERSION}-win-x64-setup.exe"
InstallDir "${INSTALL_DIR}"
RequestExecutionLevel user

Page directory
Page instfiles
UninstPage confirm
UninstPage instfiles

Section "Core Binaries & Extension" SecCore
  SetOutPath "$INSTDIR\bin"
  File "bin\agsd.exe"
  File "bin\ags.exe"
  File "bin\AgentStudios.exe"

  SetOutPath "$INSTDIR\extension"
  File "extension\agent-studios-vscode-1.0.0-rc2.vsix"

  SetOutPath "$INSTDIR\config"
  File "config\product.json"
  File "config\config.default.toml"

  ; Provision canonical OS data directories
  CreateDirectory "$APPDATA\AgentStudios"
  CreateDirectory "$LOCALAPPDATA\AgentStudios"
  CreateDirectory "$LOCALAPPDATA\AgentStudios\artifacts"

  IfFileExists "$APPDATA\AgentStudios\config.toml" skip_cfg
    CopyFiles "$INSTDIR\config\config.default.toml" "$APPDATA\AgentStudios\config.toml"
  skip_cfg:

  ; Register agentstudios:// protocol handler under HKCU
  WriteRegStr HKCU "Software\Classes\agentstudios" "" "URL:Agent Studios Protocol"
  WriteRegStr HKCU "Software\Classes\agentstudios" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\agentstudios\shell\open\command" "" '"$INSTDIR\bin\ags.exe" "%1"'

  WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\bin\agsd.exe"
  Delete "$INSTDIR\bin\ags.exe"
  Delete "$INSTDIR\bin\AgentStudios.exe"
  Delete "$INSTDIR\extension\agent-studios-vscode-1.0.0-rc2.vsix"
  Delete "$INSTDIR\config\product.json"
  Delete "$INSTDIR\config\config.default.toml"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR\bin"
  RMDir "$INSTDIR\extension"
  RMDir "$INSTDIR\config"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "Software\Classes\agentstudios"
  ; Note: %APPDATA%\AgentStudios and %LOCALAPPDATA%\AgentStudios user state preserved unless --PurgeUserData is specified.
SectionEnd
