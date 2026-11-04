/**
 * Agent Studios — Onboarding & Truthful Interaction Unit Tests
 * Milestone M14-UI-02: Extension Hardening & User Experience Assurance
 */

import * as assert from 'assert';
import { OnboardingWorkflow } from '../../chat/OnboardingWorkflow';

describe('OnboardingWorkflow Unit Tests', () => {
  it('renders disconnected onboarding card with retry and preview mode actions', () => {
    const html = OnboardingWorkflow.renderOnboardingCard({
      isConnected: false,
      isPreview: false,
      hasSeenOnboarding: false,
      daemonEndpoint: 'ipc://agent-studios.sock',
    });

    assert.ok(html.includes('Agent Studios Daemon Offline'), 'Should indicate daemon is offline');
    assert.ok(html.includes('Enable Developer Preview'), 'Should offer preview mode action');
    assert.ok(html.includes('Retry Connection'), 'Should offer connection retry');
    assert.ok(html.includes('ipc://agent-studios.sock'), 'Should include configured daemon endpoint');
    // Security check: must NEVER contain password or raw API key input fields
    assert.ok(!html.includes('<input type="password"'), 'Must never solicit password in webview');
    assert.ok(!html.includes('name="apiKey"'), 'Must never solicit API key in webview');
    assert.ok(html.includes('Security Notice'), 'Must display security notice regarding keys');
  });

  it('renders developer preview onboarding card with clear fixture explanation', () => {
    const html = OnboardingWorkflow.renderOnboardingCard({
      isConnected: true,
      isPreview: true,
      hasSeenOnboarding: false,
      daemonEndpoint: 'ipc://agent-studios.sock',
    });

    assert.ok(html.includes('Developer Preview Mode Active'), 'Should state preview mode is active');
    assert.ok(html.includes('fixtures without modifying your workspace files'), 'Should explain safe fixture boundary');
    assert.ok(html.includes('Connect to Daemon'), 'Should provide action to connect to daemon');
    assert.ok(html.includes('Security Note'), 'Should display security boundary note');
  });

  it('renders connected onboarding card for first run', () => {
    const html = OnboardingWorkflow.renderOnboardingCard({
      isConnected: true,
      isPreview: false,
      hasSeenOnboarding: false,
      activeSessionId: 'session-m14-prod',
    });

    assert.ok(html.includes('Connected to Agent Studios Daemon'), 'Should indicate connected state');
    assert.ok(html.includes('session-m14-prod'), 'Should include session identifier');
    assert.ok(html.includes('Get Started'), 'Should include getting started button');
  });

  it('returns empty string when onboarding has been dismissed and daemon is connected in normal mode', () => {
    const html = OnboardingWorkflow.renderOnboardingCard({
      isConnected: true,
      isPreview: false,
      hasSeenOnboarding: true,
    });

    assert.strictEqual(html, '');
  });
});
