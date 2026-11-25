/**
 * Agent Studios — Canonical Vietnamese (vi-VN) UI Localization Catalog
 * Milestone M15.1: First-Class Bilingual English & Vietnamese Localization
 *
 * Uses natural, professional Vietnamese developer terminology in NFC Unicode.
 */
/* eslint-disable @typescript-eslint/naming-convention */

import { TranslationCatalog } from './en';

export const VI_VN_LOCALE = 'vi-VN' as const;
export const VI_BASE_LOCALE = 'vi' as const;

export const VI_VN_CATALOG: TranslationCatalog = {
  // Mode Switching & Workbench Shell
  'mode.agentMode': 'Chế độ Tác tử',
  'mode.ideMode': 'Chế độ IDE',
  'mode.openInIde': 'Mở trong Chế độ IDE',
  'mode.switchToAgentMode': 'Chuyển sang Chế độ Tác tử',
  'mode.missionControl': 'Trung tâm Điều hành',
  'mode.openMissionControl': 'Mở Trung tâm Điều hành',

  // Agent Mode Startup Screen & Welcome Surface
  'startup.welcomeTitle': 'Không gian Kỹ thuật AI Ưu tiên Tác tử',
  'startup.welcomeSubtitle': 'Thiết lập mục tiêu kỹ thuật, điều phối đa tác tử và kiểm duyệt Bản vá (Diff) đã xác thực.',
  'startup.goalStartersHeading': 'Gợi ý Khởi tạo Mục tiêu',
  'startup.starterAuditArchitecture': 'Kiểm tra kiến trúc kho mã và xác định các điểm nghẽn phụ thuộc',
  'startup.starterDiagnoseTests': 'Chẩn đoán các kiểm thử tích hợp bị lỗi và đề xuất bản sửa tối giản',
  'startup.starterImplementFeature': 'Triển khai tính năng với xác thực trên Nhánh công việc (Worktree) cô lập',
  'startup.activeWorkspaceCard': 'Không gian làm việc Hiện tại',
  'startup.noWorkspaceSelected': 'Chưa chọn thư mục Không gian làm việc',
  'startup.sessionContinuityCard': 'Tiếp nối Phiên làm việc',
  'startup.continuePreviousSession': 'Tiếp tục Phiên làm việc Trước ({sessionId})',

  // Composer & Context Controls
  'composer.placeholder': 'Mô tả mục tiêu kỹ thuật hoặc tác vụ cho Điều phối viên...',
  'composer.submitTask': 'Gửi Mục tiêu',
  'composer.cancelRun': 'Hủy Lượt chạy Hiện tại',
  'composer.attachContext': 'Đính kèm Ngữ cảnh',
  'composer.workspaceSelectorLabel': 'Không gian làm việc',
  'composer.workspaceSelectorTooltip': 'Chọn thư mục gốc Không gian làm việc ({workspace})',
  'composer.modelSelectorLabel': 'Mô hình',
  'composer.modelSelectorTooltip': 'Chọn bộ điều hợp mô hình cho điều phối viên hoặc tác tử',
  'composer.runtimeSelectorLabel': 'Môi trường Thực thi',
  'composer.runtimeSelectorTooltip': 'Chọn nền tảng môi trường thực thi tác tử',
  'composer.sessionHistoryLabel': 'Lịch sử Phiên làm việc',
  'composer.sessionHistoryEmpty': 'Chưa có phiên làm việc nào được lưu trong Không gian làm việc này',
  'composer.sessionHistoryCount': 'Có {count} phiên làm việc đã lưu',

  // Mission Control 9 Canonical Tabs
  'missionControl.title': 'Trung tâm Điều hành',
  'missionControl.tab.overview': 'Tổng quan',
  'missionControl.tab.fleet': 'Đội ngũ Tác tử',
  'missionControl.tab.tasks': 'Cây tác vụ (DAG)',
  'missionControl.tab.runs': 'Lượt chạy',
  'missionControl.tab.timeline': 'Dòng thời gian',
  'missionControl.tab.review': 'Kiểm duyệt & Bản vá (Diff)',
  'missionControl.tab.approvals': 'Phê duyệt',
  'missionControl.tab.providers': 'Nhà cung cấp Mô hình',
  'missionControl.tab.diagnostics': 'Chẩn đoán Hệ thống',

  // Mission Control Views & Domain Concepts
  'missionControl.taskGraphTitle': 'Cây tác vụ (DAG)',
  'missionControl.worktreeLabel': 'Nhánh công việc (Worktree)',
  'missionControl.artifactDiffLabel': 'Bản vá (Diff)',
  'missionControl.activeTasksSummary': '{count} tác vụ đang hoạt động trong Cây tác vụ (DAG)',
  'missionControl.taskStatusSummary': 'Tác vụ {taskId}: {status}',

  // Approval Risk Tiers & 2PC Operator Confirmation Modal
  'approvals.inboxTitle': 'Hộp thư Chờ Phê duyệt',
  'approvals.emptyInbox': 'Không có yêu cầu Phê duyệt nào đang chờ',
  'approvals.approveAction': 'Phê duyệt',
  'approvals.denyAction': 'Từ chối',
  'approvals.riskTier.low': 'Rủi ro Thấp — Thao tác Chỉ đọc',
  'approvals.riskTier.medium': 'Rủi ro Trung bình — Thay đổi trong Phạm vi Không gian làm việc',
  'approvals.riskTier.high': 'Rủi ro Cao — Lệnh Phá hủy hoặc Tác động Bên ngoài',
  'approvals.riskTier.critical': 'Rủi ro Nghiêm trọng — Thao tác Đặc quyền Hệ thống hoặc Vượt Ranh giới',
  'approvals.modal.twoPhaseTitle': 'Yêu cầu Xác nhận Điều hành Hai Pha (2PC)',
  'approvals.modal.twoPhasePrompt': 'Kiểm tra kỹ thao tác rủi ro cao của tác vụ {taskId} ({riskTier}) trước khi cấp quyền thực thi.',
  'approvals.modal.commandPreview': 'Lệnh sẽ thực thi: {command}',
  'approvals.modal.confirmCommit': 'Cấp quyền & Thực thi',
  'approvals.modal.abortAction': 'Hủy bỏ & Từ chối Yêu cầu',
  'approvals.modal.expiredNotice': 'Mã xác thực Phê duyệt đã hết hạn; vui lòng yêu cầu mã xác nhận mới.',

  // First-Run Onboarding Wizard
  'onboarding.welcomeHeader': 'Chào mừng đến với Agent Studios',
  'onboarding.daemonOfflineTitle': 'Tiến trình nền Agent Studios Đang Ngoại tuyến',
  'onboarding.daemonOfflineBody': 'Không thể kết nối tới tiến trình nền tại {endpoint}. Hãy khởi động agsd hoặc bật Chế độ Xem trước Nhà phát triển.',
  'onboarding.daemonConnectedTitle': 'Đã Kết nối tới Tiến trình nền Agent Studios',
  'onboarding.daemonConnectedBody': 'Phiên làm việc {sessionId} đã được đồng bộ với trạng thái tiến trình nền chính thức.',
  'onboarding.previewModeTitle': 'Chế độ Xem trước Nhà phát triển Đang Bật',
  'onboarding.previewModeBody': 'Trải nghiệm giao diện với dữ liệu mẫu cố định mà không làm thay đổi tệp trong Không gian làm việc.',
  'onboarding.enablePreviewButton': 'Bật Chế độ Xem trước',
  'onboarding.connectDaemonButton': 'Kết nối Tiến trình nền',
  'onboarding.retryConnectionButton': 'Thử Kết nối Lại',
  'onboarding.getStartedButton': 'Bắt đầu Sử dụng',
  'onboarding.securityNoticeTitle': 'Thông báo Bảo mật',
  'onboarding.securityNoticeBody': 'Thông tin xác thực nhà cung cấp được quản lý độc quyền bởi chuỗi khóa hệ điều hành và CLI; tuyệt đối không nhập khóa API trực tiếp trên giao diện.',

  // Diagnostics & System Status Messages
  'diagnostics.statusHealthy': 'Tiến trình nền & Kết nối IPC Hoạt động Tốt',
  'diagnostics.statusDegraded': 'Kết nối Tiến trình nền Suy giảm — Đang Kết nối lại',
  'diagnostics.statusDisconnected': 'Đã Ngắt kết nối khỏi Tiến trình nền Chính thức',
  'diagnostics.protocolMismatch': 'Phát hiện phiên bản giao thức IPC không tương thích với tiến trình nền tại {endpoint}.',
  'diagnostics.sqliteStateHealth': 'Kho trạng thái SQLite chính thức đã được xác thực tại {path}.',
  'diagnostics.artifactStoreHealth': 'Kho tạo phẩm định danh theo nội dung đã được xác thực ({count} tạo phẩm).',
};

export default VI_VN_CATALOG;
