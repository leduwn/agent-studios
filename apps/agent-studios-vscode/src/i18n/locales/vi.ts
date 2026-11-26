/**
 * Agent Studios — Canonical Vietnamese (vi-VN) UI Localization Catalog
 * Milestone M15.1 + MM07: Bilingual English & Vietnamese Localization
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
  'startup.welcomeTitle': 'Bạn muốn làm gì hôm nay?',
  'startup.welcomeSubtitle': 'Đặt câu hỏi, lập kế hoạch thay đổi hoặc để Chế độ Tác tử chỉnh sửa và kiểm thử Không gian làm việc.',
  'startup.goalStartersHeading': 'Gợi ý Bắt đầu',
  'startup.starterAuditArchitecture': 'Giải thích cấu trúc Không gian làm việc và vị trí logic cốt lõi',
  'startup.starterDiagnoseTests': 'Xem lại các thay đổi gần đây và tìm các trường hợp biên hoặc kiểm thử còn thiếu',
  'startup.starterImplementFeature': 'Triển khai tính năng với xác thực trên Nhánh công việc (Worktree) cô lập',
  'startup.activeWorkspaceCard': 'Không gian làm việc',
  'startup.noWorkspaceSelected': 'Chưa mở thư mục',
  'startup.sessionContinuityCard': 'Lịch sử Trò chuyện',
  'startup.continuePreviousSession': 'Tiếp tục Hội thoại Trước ({sessionId})',

  // Header, Composer & Context Controls
  'header.newChat': 'Hội thoại Mới',
  'header.history': 'Lịch sử',
  'header.settings': 'Cài đặt Nhà cung cấp & Mô hình',
  'header.missionControl': 'Trung tâm Điều hành',
  'composer.placeholder': 'Đặt câu hỏi, lập kế hoạch hoặc mô tả tác vụ…',
  'composer.submitTask': 'Gửi',
  'composer.cancelRun': 'Dừng',
  'composer.attachContext': 'Đính kèm Ngữ cảnh',
  'composer.workspaceSelectorLabel': 'Không gian làm việc',
  'composer.workspaceSelectorTooltip': 'Thư mục gốc Không gian làm việc hiện tại ({workspace})',
  'composer.modelSelectorLabel': 'Mô hình',
  'composer.modelSelectorTooltip': 'Chọn mô hình AI đang sử dụng',
  'composer.runtimeSelectorLabel': 'Môi trường Thực thi',
  'composer.runtimeSelectorTooltip': 'Chọn môi trường thực thi',
  'composer.reasoningSelectorLabel': 'Suy luận',
  'composer.sessionHistoryLabel': 'Hội thoại Gần đây',
  'composer.sessionHistoryEmpty': 'Chưa có hội thoại nào',
  'composer.sessionHistoryCount': 'Có {count} hội thoại đã lưu',
  'composer.modeAsk': 'Hỏi đáp',
  'composer.modePlan': 'Lập kế hoạch',
  'composer.modeAgent': 'Tác tử',

  // 9 Connection States & Action CTAs
  'status.notConfigured': 'Thêm nhà cung cấp',
  'status.starting': 'Đang khởi động…',
  'status.connecting': 'Đang kết nối…',
  'status.ready': 'Sẵn sàng',
  'status.running': 'Đang xử lý…',
  'status.waitingForApproval': 'Cần phê duyệt',
  'status.reconnecting': 'Đang kết nối lại…',
  'status.error': 'Lỗi kết nối',
  'status.disconnected': 'Ngoại tuyến',
  'action.configureProvider': 'Cấu hình Nhà cung cấp',
  'action.openFolder': 'Mở Thư mục',
  'action.connect': 'Kết nối',
  'action.retry': 'Thử Kết nối Lại',

  // Providers & Models Settings
  'settings.title': 'Nhà cung cấp & Mô hình',
  'settings.subtitle': 'Cấu hình nhà cung cấp đám mây hoặc mô hình cục bộ. Khóa API được lưu an toàn trong VS Code SecretStorage.',
  'settings.addProvider': 'Thêm Nhà cung cấp',
  'settings.discoverModels': 'Tìm Mô hình',
  'settings.testConnection': 'Kiểm tra Kết nối',
  'settings.saveProvider': 'Lưu Nhà cung cấp',

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
  'approvals.inboxTitle': 'Yêu cầu Chờ Phê duyệt',
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

  // Clean First-Run Onboarding
  'onboarding.welcomeHeader': 'Thiết lập Agent Studios',
  'onboarding.daemonOfflineTitle': 'Động cơ Trợ lý Đang Ngoại tuyến',
  'onboarding.daemonOfflineBody': 'Khởi động hoặc kết nối động cơ trợ lý tại {endpoint} để bắt đầu.',
  'onboarding.daemonConnectedTitle': 'Đã Kết nối Động cơ Trợ lý',
  'onboarding.daemonConnectedBody': 'Hội thoại {sessionId} đã sẵn sàng.',
  'onboarding.connectDaemonButton': 'Kết nối',
  'onboarding.retryConnectionButton': 'Thử Kết nối Lại',
  'onboarding.getStartedButton': 'Cấu hình Nhà cung cấp',
  'onboarding.securityNoticeTitle': 'Lưu trữ Bảo mật',
  'onboarding.securityNoticeBody': 'Khóa API được lưu mã hóa trong VS Code SecretStorage và đồng bộ an toàn tới tiến trình nền.',

  // Diagnostics & System Status Messages
  'diagnostics.statusHealthy': 'Động cơ & Kết nối IPC Sẵn sàng',
  'diagnostics.statusDegraded': 'Kết nối Suy giảm — Đang Kết nối lại',
  'diagnostics.statusDisconnected': 'Ngoại tuyến',
  'diagnostics.protocolMismatch': 'Phát hiện phiên bản giao thức IPC không tương thích tại {endpoint}.',
  'diagnostics.sqliteStateHealth': 'Kho trạng thái SQLite chính thức đã được xác thực tại {path}.',
  'diagnostics.artifactStoreHealth': 'Kho tạo phẩm định danh theo nội dung đã được xác thực ({count} tạo phẩm).',
};

export default VI_VN_CATALOG;
