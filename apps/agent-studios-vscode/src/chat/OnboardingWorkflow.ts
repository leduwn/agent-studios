/**
 * Agent Studios — First-Run & Connection Onboarding Workflow
 * Milestone M14-UI-02: Extension Hardening & User Experience Assurance
 *
 * Implements truthful onboarding state cards explaining daemon status, diagnostics,
 * preview mode, and security boundaries without soliciting raw API keys.
 */

export interface OnboardingState {
  isConnected: boolean;
  isPreview: boolean;
  hasSeenOnboarding: boolean;
  daemonEndpoint?: string;
  activeSessionId?: string;
}

export class OnboardingWorkflow {
  /**
   * Generates the onboarding banner HTML rendered at the top of the chat view when appropriate.
   */
  public static renderOnboardingCard(state: OnboardingState): string {
    if (state.hasSeenOnboarding && state.isConnected && !state.isPreview) {
      return '';
    }

    if (state.isPreview) {
      return `
        <div class="onboarding-card onboarding-card--preview" id="onboardingBanner" role="region" aria-label="Developer Preview Status">
          <div class="onboarding-header">
            <span class="codicon codicon-beaker onboarding-icon"></span>
            <div class="onboarding-title-group">
              <span class="onboarding-title">Developer Preview Mode Active</span>
              <span class="onboarding-subtitle">Realistic simulated coordinator and fixture tasks are active</span>
            </div>
            <button class="onboarding-dismiss-btn" onclick="dismissOnboarding()" title="Dismiss banner" aria-label="Dismiss banner">
              <span class="codicon codicon-close"></span>
            </button>
          </div>
          <div class="onboarding-body">
            <p>You are viewing Agent Studios in offline preview mode. All tools, diff reviews, and session tasks run against safe local in-memory fixtures without modifying your workspace files.</p>
            <div class="onboarding-security-note">
              <span class="codicon codicon-shield"></span>
              <span><strong>Security Note:</strong> Agent Studios never prompts for raw API keys in webviews. Model provider credentials and keys are configured authoritatively in the native daemon.</span>
            </div>
            <div class="onboarding-actions">
              <button class="btn btn-secondary btn-sm" onclick="connectToDaemon()">
                <span class="codicon codicon-plug"></span> Connect to Daemon
              </button>
              <button class="btn btn-ghost btn-sm" onclick="openTaskGraph()">
                <span class="codicon codicon-type-hierarchy"></span> View Task Graph
              </button>
            </div>
          </div>
        </div>
      `;
    }

    if (!state.isConnected) {
      return `
        <div class="onboarding-card onboarding-card--disconnected" id="onboardingBanner" role="region" aria-label="Daemon Offline Notice">
          <div class="onboarding-header">
            <span class="codicon codicon-warning onboarding-icon"></span>
            <div class="onboarding-title-group">
              <span class="onboarding-title">Agent Studios Daemon Offline</span>
              <span class="onboarding-subtitle">Disconnected from native IPC control plane (${escapeHtml(state.daemonEndpoint || 'ipc://agent-studios.sock')})</span>
            </div>
            <button class="onboarding-dismiss-btn" onclick="dismissOnboarding()" title="Dismiss banner" aria-label="Dismiss banner">
              <span class="codicon codicon-close"></span>
            </button>
          </div>
          <div class="onboarding-body">
            <p>The native Agent Studios daemon is not currently responding. All coordinator actions fail closed until connected. To explore the extension features immediately, switch to Developer Preview Mode.</p>
            <div class="onboarding-security-note">
              <span class="codicon codicon-shield"></span>
              <span><strong>Security Notice:</strong> Do not enter API keys or passwords into chat inputs. Configure keys securely via daemon environment variables or the system keyring.</span>
            </div>
            <div class="onboarding-actions">
              <button class="btn btn-primary btn-sm" onclick="enablePreviewMode()">
                <span class="codicon codicon-beaker"></span> Enable Developer Preview
              </button>
              <button class="btn btn-secondary btn-sm" onclick="connectToDaemon()">
                <span class="codicon codicon-sync"></span> Retry Connection
              </button>
            </div>
          </div>
        </div>
      `;
    }

    // Connected state first-run banner
    return `
      <div class="onboarding-card onboarding-card--connected" id="onboardingBanner" role="region" aria-label="Getting Started with Agent Studios">
        <div class="onboarding-header">
          <span class="codicon codicon-check onboarding-icon"></span>
          <div class="onboarding-title-group">
            <span class="onboarding-title">Connected to Agent Studios Daemon</span>
            <span class="onboarding-subtitle">IPC channel active — session ${escapeHtml(state.activeSessionId || 'default')}</span>
          </div>
          <button class="onboarding-dismiss-btn" onclick="dismissOnboarding()" title="Dismiss banner" aria-label="Dismiss banner">
            <span class="codicon codicon-close"></span>
          </button>
        </div>
        <div class="onboarding-body">
          <p>Choose an agent mode below to begin: <strong>Ask</strong> for fast explanations, <strong>Plan</strong> for multi-step architectural blueprints, or <strong>Agent</strong> for autonomous code editing with human-in-the-loop approvals.</p>
          <div class="onboarding-actions">
            <button class="btn btn-secondary btn-sm" onclick="dismissOnboarding()">
              Get Started
            </button>
          </div>
        </div>
      </div>
    `;
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}
