import { X, Edit3 } from 'lucide-react';
import type { PlanEditStats } from '../../types/plan-edit';

interface PlanEditHeaderProps {
  stats: PlanEditStats;
  onClose: () => void;
}

export function PlanEditHeader({ stats, onClose }: PlanEditHeaderProps) {
  return (
    <div className="flex items-center justify-between px-5 py-4 border-b border-white/10">
      <div className="flex items-center gap-3">
        <div className="w-8 h-8 rounded-lg bg-orange-500/20 flex items-center justify-center">
          <Edit3 size={16} className="text-orange-400" />
        </div>
        <div>
          <h2 id="plan-edit-title" className="text-base font-semibold text-gray-100">
            Edit Organization Plan
          </h2>
          <div className="flex items-center gap-3 text-xs text-gray-500 mt-0.5">
            <span>
              {stats.enabled} of {stats.total} operations selected
            </span>
            {stats.modified > 0 && (
              <span className="text-orange-400">{stats.modified} modified</span>
            )}
          </div>
        </div>
      </div>
      <button
        onClick={onClose}
        className="p-2 rounded-lg hover:bg-white/10 text-gray-400 hover:text-gray-200 transition-colors"
        aria-label="Close modal"
      >
        <X size={18} />
      </button>
    </div>
  );
}
