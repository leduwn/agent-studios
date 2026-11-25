/**
 * Agent Studios — UI Localization Manager & Language Decoupling Engine
 * Milestone M15.1: Bilingual Localization (en-US, vi-VN) & UI/Agent Decoupling
 *
 * Implements the deterministic fallback hierarchy from docs/agent-studios/LOCALIZATION.md:
 *   User Selected Locale (e.g., vi-VN)
 *     -> Base Language Bundle (vi)
 *     -> Canonical Default (en-US)
 *     -> Raw Key Identifier ([missing: <key>])
 *
 * Enforces the core architectural invariant:
 *   UI Display Language !== Agent Response Language
 */

import { EN_BASE_LOCALE, EN_US_CATALOG, EN_US_LOCALE, TranslationKey } from './locales/en';
import { VI_BASE_LOCALE, VI_VN_CATALOG, VI_VN_LOCALE } from './locales/vi';

export type SupportedDisplayLocale = 'en-US' | 'vi-VN';
export type AgentResponseLanguage = 'auto' | 'en-US' | 'vi-VN';
export type InterpolationParams = Record<string, string | number>;

export type FallbackResolutionStage =
  | 'selected-locale'
  | 'base-language'
  | 'canonical-default'
  | 'missing-key';

export interface TranslationResolution {
  readonly key: string;
  readonly text: string;
  readonly resolvedFrom: FallbackResolutionStage;
  readonly resolvedLocale: string | null;
}

export interface LocalizationManagerOptions {
  readonly displayLanguage?: string;
  readonly agentResponseLanguage?: AgentResponseLanguage;
  readonly customBundles?: Record<string, Record<string, string>>;
}

export class LocalizationManager {
  private displayLanguage: string;
  private agentResponseLanguage: AgentResponseLanguage;
  private readonly bundles: Map<string, Record<string, string>>;

  constructor(options?: LocalizationManagerOptions) {
    this.displayLanguage = options?.displayLanguage ?? EN_US_LOCALE;
    this.agentResponseLanguage = options?.agentResponseLanguage ?? 'auto';
    this.bundles = new Map<string, Record<string, string>>();

    // Register canonical default and base language bundles
    this.bundles.set(EN_US_LOCALE, { ...EN_US_CATALOG });
    this.bundles.set(EN_BASE_LOCALE, { ...EN_US_CATALOG });
    this.bundles.set(VI_VN_LOCALE, { ...VI_VN_CATALOG });
    this.bundles.set(VI_BASE_LOCALE, { ...VI_VN_CATALOG });

    if (options?.customBundles) {
      for (const [locale, catalog] of Object.entries(options.customBundles)) {
        this.bundles.set(locale, { ...catalog });
      }
    }
  }

  /**
   * Sets the UI display language without mutating the agent response language.
   */
  public setDisplayLanguage(locale: string): void {
    const trimmed = locale.trim();
    this.displayLanguage = trimmed.length > 0 ? trimmed : EN_US_LOCALE;
  }

  /**
   * Returns the active UI display language locale tag.
   */
  public getDisplayLanguage(): string {
    return this.displayLanguage;
  }

  /**
   * Explicitly sets the LLM agent response language preference ('auto' | 'en-US' | 'vi-VN').
   * Does not mutate the UI display language.
   */
  public setAgentResponseLanguage(lang: AgentResponseLanguage): void {
    if (lang !== 'auto' && lang !== 'en-US' && lang !== 'vi-VN') {
      throw new Error(`Unsupported agent response language: ${String(lang)}`);
    }
    this.agentResponseLanguage = lang;
  }

  /**
   * Returns the configured agent response language preference.
   */
  public getAgentResponseLanguage(): AgentResponseLanguage {
    return this.agentResponseLanguage;
  }

  /**
   * Registers or replaces a locale bundle (useful for regional overrides or testing partial bundles).
   */
  public registerBundle(locale: string, catalog: Record<string, string>): void {
    this.bundles.set(locale, { ...catalog });
  }

  /**
   * Extracts the ISO 639-1 base language code from a BCP-47 locale tag (e.g., 'vi-VN' -> 'vi').
   */
  public static extractBaseLanguage(locale: string): string {
    const separatorIndex = locale.indexOf('-');
    if (separatorIndex === -1) {
      return locale.toLowerCase();
    }
    return locale.slice(0, separatorIndex).toLowerCase();
  }

  /**
   * Resolves a translation key through the 4-stage deterministic fallback chain:
   *   1. User Selected Locale (e.g. 'vi-VN')
   *   2. Base Language Bundle (e.g. 'vi')
   *   3. Canonical Default ('en-US')
   *   4. Raw Key Identifier ('[missing: <key>]')
   */
  public resolveTranslation(
    key: TranslationKey | string,
    params?: InterpolationParams
  ): TranslationResolution {
    const selectedLocale = this.displayLanguage;

    // Stage 1: Exact selected locale bundle
    const selectedBundle = this.bundles.get(selectedLocale);
    if (
      selectedBundle &&
      Object.prototype.hasOwnProperty.call(selectedBundle, key) &&
      typeof selectedBundle[key] === 'string'
    ) {
      return {
        key,
        text: LocalizationManager.interpolate(selectedBundle[key], params),
        resolvedFrom: 'selected-locale',
        resolvedLocale: selectedLocale,
      };
    }

    // Stage 2: Base language bundle (e.g. 'vi-VN' -> 'vi')
    const baseLocale = LocalizationManager.extractBaseLanguage(selectedLocale);
    if (baseLocale !== selectedLocale) {
      const baseBundle = this.bundles.get(baseLocale);
      if (
        baseBundle &&
        Object.prototype.hasOwnProperty.call(baseBundle, key) &&
        typeof baseBundle[key] === 'string'
      ) {
        return {
          key,
          text: LocalizationManager.interpolate(baseBundle[key], params),
          resolvedFrom: 'base-language',
          resolvedLocale: baseLocale,
        };
      }
    }

    // Stage 3: Canonical default ('en-US')
    if (selectedLocale !== EN_US_LOCALE) {
      const defaultBundle = this.bundles.get(EN_US_LOCALE);
      if (
        defaultBundle &&
        Object.prototype.hasOwnProperty.call(defaultBundle, key) &&
        typeof defaultBundle[key] === 'string'
      ) {
        return {
          key,
          text: LocalizationManager.interpolate(defaultBundle[key], params),
          resolvedFrom: 'canonical-default',
          resolvedLocale: EN_US_LOCALE,
        };
      }
    }

    // Stage 4: Raw key identifier fallback
    return {
      key,
      text: `[missing: ${key}]`,
      resolvedFrom: 'missing-key',
      resolvedLocale: null,
    };
  }

  /**
   * Translates a key using the active display language and interpolates optional parameters.
   */
  public t(key: TranslationKey | string, params?: InterpolationParams): string {
    return this.resolveTranslation(key, params).text;
  }

  /**
   * Alias for `t(key, params)`.
   */
  public translate(key: TranslationKey | string, params?: InterpolationParams): string {
    return this.t(key, params);
  }

  /**
   * Interpolates `{paramName}` placeholders in a template string.
   */
  public static interpolate(template: string, params?: InterpolationParams): string {
    if (!params) {
      return template;
    }
    return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (fullMatch, tokenName: string) => {
      if (Object.prototype.hasOwnProperty.call(params, tokenName)) {
        const value = params[tokenName];
        return value !== undefined && value !== null ? String(value) : fullMatch;
      }
      return fullMatch;
    });
  }

  /**
   * Resolves the effective prompt sent to the cognitive Coordinator / Agent runtime.
   *
   * Strict Decoupling Invariant:
   * - Changing `displayLanguage` NEVER mutates or forces the LLM response language.
   * - When `agentResponseLanguage === 'auto'`, the user prompt is returned completely untouched
   *   so the Coordinator naturally mirrors the user's conversational language.
   * - Only when `agentResponseLanguage` is explicitly set to `'en-US'` or `'vi-VN'` is a
   *   response-language directive appended.
   */
  public resolveEffectiveAgentPrompt(userPrompt: string): string {
    if (this.agentResponseLanguage === 'auto') {
      return userPrompt;
    }
    return `${userPrompt}\n\n[Agent Response Language: ${this.agentResponseLanguage}]`;
  }

  /**
   * Verifies that UI Display Language and Agent Response Language are strictly orthogonal.
   */
  public verifyLanguageDecouplingInvariant(): boolean {
    const savedDisplay = this.displayLanguage;
    const savedAgentLang = this.agentResponseLanguage;

    try {
      const sampleViPrompt = 'Phân tích kiến trúc hệ thống và viết tài liệu';
      const sampleEnPrompt = 'Audit repository architecture and document findings';

      // 1. With agentResponseLanguage = 'auto', toggling displayLanguage must NEVER alter prompt
      this.setAgentResponseLanguage('auto');

      this.setDisplayLanguage('en-US');
      const autoWithEnUi = this.resolveEffectiveAgentPrompt(sampleViPrompt);
      const enUiLabel = this.t('mode.agentMode');

      this.setDisplayLanguage('vi-VN');
      const autoWithViUi = this.resolveEffectiveAgentPrompt(sampleViPrompt);
      const viUiLabel = this.t('mode.agentMode');

      if (autoWithEnUi !== sampleViPrompt || autoWithViUi !== sampleViPrompt) {
        return false;
      }
      if (autoWithEnUi !== autoWithViUi) {
        return false;
      }
      if (enUiLabel === viUiLabel) {
        return false;
      }

      // 2. With agentResponseLanguage = 'en-US', toggling displayLanguage must produce identical agent prompts
      this.setAgentResponseLanguage('en-US');
      this.setDisplayLanguage('vi-VN');
      const forcedEnWithViUi = this.resolveEffectiveAgentPrompt(sampleViPrompt);
      const viUiLabelWhenAgentEn = this.t('mode.agentMode');

      this.setDisplayLanguage('en-US');
      const forcedEnWithEnUi = this.resolveEffectiveAgentPrompt(sampleViPrompt);

      if (forcedEnWithViUi !== forcedEnWithEnUi) {
        return false;
      }
      if (viUiLabelWhenAgentEn !== viUiLabel) {
        return false;
      }

      // 3. With agentResponseLanguage = 'vi-VN', toggling displayLanguage must produce identical agent prompts
      this.setAgentResponseLanguage('vi-VN');
      this.setDisplayLanguage('en-US');
      const forcedViWithEnUi = this.resolveEffectiveAgentPrompt(sampleEnPrompt);
      const enUiLabelWhenAgentVi = this.t('mode.agentMode');

      this.setDisplayLanguage('vi-VN');
      const forcedViWithViUi = this.resolveEffectiveAgentPrompt(sampleEnPrompt);

      if (forcedViWithEnUi !== forcedViWithViUi) {
        return false;
      }
      if (enUiLabelWhenAgentVi !== enUiLabel) {
        return false;
      }

      return true;
    } finally {
      this.displayLanguage = savedDisplay;
      this.agentResponseLanguage = savedAgentLang;
    }
  }
}

export const defaultLocalizationManager = new LocalizationManager();
