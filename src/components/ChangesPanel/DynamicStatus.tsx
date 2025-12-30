import { useMemo } from 'react';
import { Loader2 } from 'lucide-react';
import type { ThoughtType } from '../../stores/organize-store';

interface DynamicStatusProps {
  /** The type of event currently happening */
  eventType: ThoughtType | string;
  /** Additional detail from the event (e.g., search query, file count) */
  detail?: string | null;
}

/**
 * Status phrases for each event type.
 * These rotate when no specific detail is available.
 */
const STATUS_PHRASES: Record<string, string[]> = {
  scanning: [
    'Scanning folder structure',
    'Reading file metadata',
    'Building file index',
  ],
  indexing: [
    'Scanning folder structure',
    'Reading file metadata',
    'Building file index',
  ],
  analyzing: [
    'Analyzing file contents',
    'Understanding patterns',
    'Examining structure',
  ],
  searching: [
    'Searching files',
    'Finding matches',
    'Analyzing content',
  ],
  applying_rules: [
    'Applying organization rules',
    'Matching file patterns',
    'Planning moves',
  ],
  previewing: [
    'Generating preview',
    'Calculating changes',
  ],
  planning: [
    'Creating organization plan',
    'Mapping file destinations',
    'Structuring folders',
  ],
  thinking: [
    'Reasoning about structure',
    'Analyzing patterns',
    'Planning organization',
  ],
  committing: [
    'Finalizing plan',
    'Preparing changes',
  ],
  executing: [
    'Applying changes',
    'Moving files',
    'Organizing folder',
  ],
  naming_conventions: [
    'Detecting naming patterns',
    'Analyzing file names',
  ],
};

/**
 * Get a rotating phrase for the given event type.
 * Uses timestamp to cycle through phrases.
 */
function getRotatingPhrase(eventType: string): string {
  const phrases = STATUS_PHRASES[eventType] || STATUS_PHRASES.thinking;
  const index = Math.floor(Date.now() / 2000) % phrases.length;
  return phrases[index];
}

/**
 * Extract contextual info from the detail string.
 */
function parseDetail(eventType: string, detail: string | null | undefined): string | null {
  if (!detail) return null;

  // "Searching for 'query'" -> extract query
  if (eventType === 'searching' && detail.includes("'")) {
    const match = detail.match(/'([^']+)'/);
    if (match) return match[1];
  }

  // "Found X files" -> pass through
  if (detail.includes('Found') && detail.includes('files')) {
    return detail;
  }

  // "Applying N rules" -> extract count
  if (eventType === 'applying_rules' && detail.includes('rules')) {
    const match = detail.match(/(\d+)\s*rules/);
    if (match) return `${match[1]} rules`;
  }

  // "Plan created with N operations"
  if (detail.includes('operations')) {
    const match = detail.match(/(\d+)\s*operations/);
    if (match) return `${match[1]} operations`;
  }

  return null;
}

/**
 * Build the display message based on event type and detail.
 */
function buildMessage(eventType: string, detail: string | null | undefined): string {
  const parsed = parseDetail(eventType, detail);

  // Use parsed detail for contextual messages
  if (parsed) {
    switch (eventType) {
      case 'searching':
        return `Searching for "${parsed}"`;
      case 'applying_rules':
        return `Applying ${parsed}`;
      case 'indexing':
      case 'scanning':
        if (parsed.includes('Found')) return parsed;
        break;
      case 'committing':
        if (parsed.includes('operations')) {
          return `Creating plan with ${parsed}`;
        }
        break;
    }
  }

  // Fall back to rotating phrases
  return getRotatingPhrase(eventType);
}

/**
 * Dynamic status indicator showing Cursor-style loading text.
 * Displays contextual messages based on the current agent activity.
 */
export function DynamicStatus({ eventType, detail }: DynamicStatusProps) {
  const message = useMemo(
    () => buildMessage(eventType, detail),
    [eventType, detail]
  );

  return (
    <div className="flex items-center gap-2">
      <Loader2 size={12} className="text-orange-400 animate-spin" />
      <p className="text-xs text-gray-300 dynamic-status-text">
        {message}
        <span className="loading-dots" />
      </p>
    </div>
  );
}
