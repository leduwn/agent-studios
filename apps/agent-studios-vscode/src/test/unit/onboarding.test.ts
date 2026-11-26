/**
 * Agent Studios — Clean First-Run Onboarding Unit Tests
 */

import * as assert from 'assert';
import { OnboardingWorkflow } from '../../chat/OnboardingWorkflow';

describe('OnboardingWorkflow Unit Tests', () => {
  it('renders clean offline onboarding card with Connect CTA and zero developer preview jargon', () => {
    const html = OnboardingWorkflow.renderBanner({
      isConnected: false,
      connectionState: 'disconnected',
      hasProviders: false,
      hasWorkspace: false,
      hasDismissed: false,
    });

    assert.ok(html.includes('Engine is offline'), 'Should indicate engine is offline cleanly');
    assert.ok(html.includes('Connect'), 'Should offer Connect action');
    assert.ok(!html.includes('Developer Preview'), 'Must NEVER mention Developer Preview');
    assert.ok(!html.includes('ipc://'), 'Must NEVER expose raw IPC paths');
    assert.ok(!html.includes('<input type="password"'), 'Must never embed password input in chat onboarding');
  });

  it('renders provider configuration onboarding card when engine is connected but no provider configured', () => {
    const html = OnboardingWorkflow.renderBanner({
      isConnected: true,
      connectionState: 'connected',
      hasProviders: false,
      hasWorkspace: true,
      hasDismissed: false,
    });

    assert.ok(html.includes('Add a model provider'), 'Should ask user to add a provider');
    assert.ok(html.includes('Configure Provider'), 'Should include Configure Provider CTA');
    assert.ok(!html.includes('Developer Preview'));
  });

  it('returns empty string when engine is connected and provider is configured', () => {
    const html = OnboardingWorkflow.renderBanner({
      isConnected: true,
      connectionState: 'connected',
      hasProviders: true,
      hasWorkspace: true,
      hasDismissed: false,
    });

    assert.strictEqual(html, '');
  });

  it('returns empty string when onboarding has been dismissed', () => {
    const html = OnboardingWorkflow.renderBanner({
      isConnected: false,
      connectionState: 'disconnected',
      hasProviders: false,
      hasWorkspace: false,
      hasDismissed: true,
    });

    assert.strictEqual(html, '');
  });
});
