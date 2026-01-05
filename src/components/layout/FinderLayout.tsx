import { useState, useEffect } from 'react';
import { Sidebar } from './Sidebar';
import { Toolbar } from './Toolbar';
import { MainView } from './MainView';
import { QuickLook } from '../preview/QuickLook';
import { SettingsPanel } from '../Settings/SettingsPanel';
import { ChangesPanel } from '../ChangesPanel/ChangesPanel';
import { ChatPanel } from '../ChatPanel/ChatPanel';
import { HistoryPanel } from '../HistoryPanel/HistoryPanel';
import { useOrganizeStore } from '../../stores/organize-store';
import { useNavigationStore } from '../../stores/navigation-store';
import { useHistoryStore } from '../../stores/history-store';

export function FinderLayout() {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [chatPanelOpen, setChatPanelOpen] = useState(false);
  const [historyPanelOpen, setHistoryPanelOpen] = useState(false);
  const { isOpen: isOrganizing } = useOrganizeStore();
  const { currentPath } = useNavigationStore();
  const {
    indicatorHasHistory,
    indicatorSessionCount,
    checkFolderIndicator,
    loadHistory,
  } = useHistoryStore();

  // Check for history when folder changes
  useEffect(() => {
    if (currentPath) {
      checkFolderIndicator(currentPath);
    }
  }, [currentPath, checkFolderIndicator]);

  // Load full history when panel opens
  useEffect(() => {
    if (historyPanelOpen && currentPath) {
      loadHistory(currentPath);
    }
  }, [historyPanelOpen, currentPath, loadHistory]);

  return (
    <div className="h-screen flex flex-col overflow-hidden">
      {/* Toolbar */}
      <Toolbar
        onOpenSettings={() => setSettingsOpen(true)}
        onToggleChat={() => setChatPanelOpen((prev) => !prev)}
        onToggleHistory={() => setHistoryPanelOpen((prev) => !prev)}
        historyHasContent={indicatorHasHistory}
        historySessionCount={indicatorSessionCount}
        historyPanelOpen={historyPanelOpen}
      />

      {/* Main content area */}
      <div className="flex-1 flex overflow-hidden">
        {/* Sidebar */}
        <Sidebar />

        {/* Main file view */}
        <MainView />

        {/* Quick Look preview panel (hidden when organizing) */}
        {!isOrganizing && <QuickLook />}

        {/* AI Changes panel (shown when organizing) */}
        {isOrganizing && <ChangesPanel />}

        {/* Chat panel (hidden when organizing) */}
        {!isOrganizing && (
          <ChatPanel
            isOpen={chatPanelOpen}
            onClose={() => setChatPanelOpen(false)}
          />
        )}

        {/* History panel (hidden when organizing) */}
        {!isOrganizing && historyPanelOpen && currentPath && (
          <HistoryPanel
            folderPath={currentPath}
            onClose={() => setHistoryPanelOpen(false)}
          />
        )}
      </div>

      {/* Settings panel */}
      <SettingsPanel
        isOpen={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
    </div>
  );
}
