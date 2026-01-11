import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useNavigationStore } from './navigation-store';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('NavigationStore', () => {
  beforeEach(() => {
    // Reset store state
    useNavigationStore.setState({
      currentPath: '/Users',
      history: ['/Users'],
      historyIndex: 0,
      viewMode: 'list',
      sortField: 'name',
      sortDirection: 'asc',
      showHidden: false,
      quickLookActive: false,
      quickLookPath: null,
    });
    vi.clearAllMocks();
  });

  describe('Path Navigation', () => {
    it('should have initial path', () => {
      const state = useNavigationStore.getState();
      expect(state.currentPath).toBe('/Users');
    });

    it('should navigate to new path', () => {
      const { navigateTo } = useNavigationStore.getState();
      navigateTo('/Users/Documents');

      const state = useNavigationStore.getState();
      expect(state.currentPath).toBe('/Users/Documents');
      expect(state.history).toContain('/Users/Documents');
    });

    it('should go back in history', () => {
      const { navigateTo, goBack } = useNavigationStore.getState();
      navigateTo('/Users/Documents');
      navigateTo('/Users/Downloads');

      goBack();

      const state = useNavigationStore.getState();
      expect(state.currentPath).toBe('/Users/Documents');
    });

    it('should go forward in history', () => {
      const { navigateTo, goBack, goForward } = useNavigationStore.getState();
      navigateTo('/Users/Documents');
      navigateTo('/Users/Downloads');
      goBack();

      goForward();

      const state = useNavigationStore.getState();
      expect(state.currentPath).toBe('/Users/Downloads');
    });

    it('should go up to parent directory', () => {
      useNavigationStore.setState({ currentPath: '/Users/Documents/Work' });
      const { goUp } = useNavigationStore.getState();

      goUp();

      const state = useNavigationStore.getState();
      expect(state.currentPath).toBe('/Users/Documents');
    });
  });

  describe('View Mode', () => {
    it('should change view mode to grid', () => {
      const { setViewMode } = useNavigationStore.getState();

      setViewMode('grid');
      expect(useNavigationStore.getState().viewMode).toBe('grid');
    });

    it('should change view mode to columns', () => {
      const { setViewMode } = useNavigationStore.getState();

      setViewMode('columns');
      expect(useNavigationStore.getState().viewMode).toBe('columns');
    });

    it('should change view mode to list', () => {
      useNavigationStore.setState({ viewMode: 'grid' });
      const { setViewMode } = useNavigationStore.getState();

      setViewMode('list');
      expect(useNavigationStore.getState().viewMode).toBe('list');
    });
  });

  describe('Sorting', () => {
    it('should change sort field to date', () => {
      const { setSortField } = useNavigationStore.getState();

      setSortField('date');
      expect(useNavigationStore.getState().sortField).toBe('date');
    });

    it('should change sort field to size', () => {
      const { setSortField } = useNavigationStore.getState();

      setSortField('size');
      expect(useNavigationStore.getState().sortField).toBe('size');
    });

    it('should toggle sort direction', () => {
      const { toggleSortDirection } = useNavigationStore.getState();

      expect(useNavigationStore.getState().sortDirection).toBe('asc');

      toggleSortDirection();
      expect(useNavigationStore.getState().sortDirection).toBe('desc');

      toggleSortDirection();
      expect(useNavigationStore.getState().sortDirection).toBe('asc');
    });
  });

  describe('Hidden Files', () => {
    it('should toggle show hidden files', () => {
      const { toggleShowHidden } = useNavigationStore.getState();

      expect(useNavigationStore.getState().showHidden).toBe(false);

      toggleShowHidden();
      expect(useNavigationStore.getState().showHidden).toBe(true);

      toggleShowHidden();
      expect(useNavigationStore.getState().showHidden).toBe(false);
    });
  });

  describe('Quick Look', () => {
    it('should toggle quick look', () => {
      const { toggleQuickLook } = useNavigationStore.getState();

      toggleQuickLook('/test/file.txt');
      expect(useNavigationStore.getState().quickLookActive).toBe(true);
      expect(useNavigationStore.getState().quickLookPath).toBe('/test/file.txt');
    });

    it('should close quick look', () => {
      useNavigationStore.setState({ quickLookActive: true, quickLookPath: '/test' });
      const { closeQuickLook } = useNavigationStore.getState();

      closeQuickLook();
      expect(useNavigationStore.getState().quickLookActive).toBe(false);
    });
  });
});
