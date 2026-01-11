import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useSubscriptionStore, TIER_LIMITS } from './subscription-store';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('SubscriptionStore', () => {
  beforeEach(() => {
    // Reset store state
    useSubscriptionStore.setState({
      tier: 'free',
      isLoaded: true,
    });
    vi.clearAllMocks();
  });

  describe('Tier Limits Configuration', () => {
    it('should have free tier limits defined', () => {
      expect(TIER_LIMITS.free).toBeDefined();
      expect(TIER_LIMITS.free.haiku).toBe(100);
      expect(TIER_LIMITS.free.gpt5mini).toBe(50);
      expect(TIER_LIMITS.free.gpt5nano).toBe(100);
    });

    it('should have pro tier limits defined', () => {
      expect(TIER_LIMITS.pro).toBeDefined();
      expect(TIER_LIMITS.pro.haiku).toBe(300);
      expect(TIER_LIMITS.pro.sonnet).toBe(50);
    });

    it('should block pro-only features on free tier', () => {
      expect(TIER_LIMITS.free.sonnet).toBe(0);
      expect(TIER_LIMITS.free.extendedThinking).toBe(0);
      expect(TIER_LIMITS.free.gpt52).toBe(0);
    });

    it('should have pro tier with higher limits than free', () => {
      expect(TIER_LIMITS.pro.haiku).toBeGreaterThan(TIER_LIMITS.free.haiku);
      expect(TIER_LIMITS.pro.sonnet).toBeGreaterThan(TIER_LIMITS.free.sonnet);
      expect(TIER_LIMITS.pro.gpt5mini).toBeGreaterThan(TIER_LIMITS.free.gpt5mini);
    });
  });

  describe('Tier State', () => {
    it('should have free tier by default', () => {
      const state = useSubscriptionStore.getState();
      expect(state.tier).toBe('free');
    });

    it('should update tier correctly', () => {
      useSubscriptionStore.setState({ tier: 'pro' });
      expect(useSubscriptionStore.getState().tier).toBe('pro');

      useSubscriptionStore.setState({ tier: 'free' });
      expect(useSubscriptionStore.getState().tier).toBe('free');
    });

    it('should track loaded state', () => {
      useSubscriptionStore.setState({ isLoaded: false });
      expect(useSubscriptionStore.getState().isLoaded).toBe(false);

      useSubscriptionStore.setState({ isLoaded: true });
      expect(useSubscriptionStore.getState().isLoaded).toBe(true);
    });
  });
});
