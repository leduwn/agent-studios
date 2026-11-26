/**
 * Agent Studios — Clean First-Run Onboarding Helper
 * Guides new users calmly: Connect -> Add Provider -> Start Chatting.
 * Zero developer preview fixtures or low-level IPC jargon.
 */

import { ConnectionState, ProviderSummary } from '../bridge/types';
import { escapeHtml } from '../webviews/markdownRenderer';
import { UiConnectionStatus } from './ChatPresenter';

export interface OnboardingState {
  isConnected: boolean;
  connectionState: ConnectionState;
  uiStatus?: UiConnectionStatus;
  hasProviders: boolean;
  hasWorkspace: boolean;
  hasDismissed: boolean;
}

export class OnboardingWorkflow {
  /**
   * Determines whether the compact onboarding checklist should be displayed
   * inside the empty state when the user still needs setup.
   */
  public static shouldShowBanner(state: OnboardingState): boolean {
    if (state.hasDismissed) {
      return false;
    }
    return !state.isConnected || !state.hasProviders;
  }

  /**
   * Renders a quiet, actionable 3-step getting-started card with zero dev/debug jargon.
   */
  public static renderBanner(state: OnboardingState): string {
    if (!OnboardingWorkflow.shouldShowBanner(state)) {
      return '';
    }

    const step1Done = state.isConnected;
    const step2Done = state.hasProviders;
    const step3Done = state.hasWorkspace;

    let headline = 'Set up Agent Studios';
    let subtext = 'Connect the local assistant engine and add a model provider to start coding.';
    let primaryBtnId = 'onboardingConfigureBtn';
    let primaryAction = 'configureProvider';
    let primaryLabel = 'Configure Provider';
    let secondaryHtml = '';

    if (!state.isConnected) {
      if (state.connectionState === 'connecting' || state.connectionState === 'reconnecting') {
        headline = 'Connecting engine…';
        subtext = 'Starting the local assistant engine for this workspace.';
      } else {
        headline = 'Engine is offline';
        subtext = 'Start or connect the local assistant engine to begin.';
      }
      primaryBtnId = 'onboardingConnectBtn';
      primaryAction = 'connectDaemon';
      primaryLabel = 'Connect';
      secondaryHtml = `<button class="btn-ghost btn-sm" data-onboarding-action="openSettings">Provider Settings</button>`;
    } else if (!state.hasProviders) {
      headline = 'Add a model provider';
      subtext = 'Choose OpenAI, Anthropic, Gemini, DeepSeek, Ollama, or any custom endpoint.';
      primaryBtnId = 'onboardingConfigureBtn';
      primaryAction = 'openSettings';
      primaryLabel = 'Configure Provider';
      if (!state.hasWorkspace) {
        secondaryHtml = `<button class="btn-ghost btn-sm" data-onboarding-action="openFolder">Open Folder</button>`;
      }
    }

    return `
      <section class="onboarding-card" role="region" aria-label="Getting Started">
        <div class="onboarding-card-header">
          <div>
            <div class="onboarding-title">${escapeHtml(headline)}</div>
            <div class="onboarding-subtitle">${escapeHtml(subtext)}</div>
          </div>
          <button id="dismissOnboardingBtn" class="icon-btn" title="Dismiss" aria-label="Dismiss setup">✕</button>
        </div>
        <ol class="onboarding-steps">
          <li class="onboarding-step ${step1Done ? 'done' : 'current'}">
            <span class="step-mark">${step1Done ? '✓' : '1'}</span>
            <span>Engine connected</span>
          </li>
          <li class="onboarding-step ${step2Done ? 'done' : step1Done ? 'current' : ''}">
            <span class="step-mark">${step2Done ? '✓' : '2'}</span>
            <span>Provider &amp; model configured</span>
          </li>
          <li class="onboarding-step ${step3Done ? 'done' : ''}">
            <span class="step-mark">${step3Done ? '✓' : '3'}</span>
            <span>Workspace folder${step3Done ? '' : ' (optional for Ask/Plan)'}</span>
          </li>
        </ol>
        <div class="onboarding-actions">
          <button id="${primaryBtnId}" class="btn-primary btn-sm" data-onboarding-action="${primaryAction}">
            ${escapeHtml(primaryLabel)}
          </button>
          ${secondaryHtml}
        </div>
      </section>
    `;
  }

  public static hasConfiguredProviders(providers?: readonly ProviderSummary[]): boolean {
    if (!providers || providers.length === 0) {
      return false;
    }
    return providers.some((p) => p.status !== 'unconfigured');
  }
}
