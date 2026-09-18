import { describe, expect, it } from 'vitest';
import { resolveAppLanguage } from './i18n';

describe('resolveAppLanguage', () => {
  it('keeps an explicitly stored Simplified Chinese preference', () => {
    expect(resolveAppLanguage('en-US', 'zh-CN')).toBe('zh-CN');
  });

  it.each(['zh-CN', 'zh-SG', 'zh-Hans', 'zh-Hans-CN', 'zh'])(
    'detects %s as Simplified Chinese',
    (language) => {
      expect(resolveAppLanguage(language)).toBe('zh-CN');
    },
  );

  it('does not silently map Traditional Chinese locales to Simplified Chinese', () => {
    expect(resolveAppLanguage('zh-TW')).toBe('en');
    expect(resolveAppLanguage('zh-HK')).toBe('en');
  });

  it('keeps existing English and Vietnamese behavior', () => {
    expect(resolveAppLanguage('en-US')).toBe('en');
    expect(resolveAppLanguage('vi-VN')).toBe('vi');
  });
});
