import { useRef, useEffect, useState, useCallback, useMemo } from 'react';
import { cn } from '../../lib/utils';

/**
 * Security validation result for filenames
 */
interface ValidationResult {
  valid: boolean;
  error?: string;
}

/**
 * Validate a filename for security issues
 * Blocks path traversal, control characters, and reserved names
 */
function validateFilename(name: string): ValidationResult {
  // Empty or whitespace-only
  if (!name || !name.trim()) {
    return { valid: false, error: 'Name cannot be empty' };
  }

  const trimmed = name.trim();

  // Path separators - prevent directory traversal
  if (trimmed.includes('/') || trimmed.includes('\\')) {
    return { valid: false, error: 'Name cannot contain path separators (/ or \\)' };
  }

  // Parent directory reference
  if (trimmed === '..' || trimmed.startsWith('../') || trimmed.includes('/../')) {
    return { valid: false, error: 'Invalid directory reference' };
  }

  // Hidden file that's just dots
  if (/^\.+$/.test(trimmed)) {
    return { valid: false, error: 'Invalid filename' };
  }

  // Control characters (ASCII 0-31 and 127)
  if (/[\x00-\x1f\x7f]/.test(trimmed)) {
    return { valid: false, error: 'Name cannot contain control characters' };
  }

  // Null bytes
  if (trimmed.includes('\0')) {
    return { valid: false, error: 'Name cannot contain null characters' };
  }

  // Reserved characters on Windows (still block for cross-platform safety)
  if (/[<>:"|?*]/.test(trimmed)) {
    return { valid: false, error: 'Name contains reserved characters' };
  }

  // Very long names (filesystem limit is typically 255)
  if (trimmed.length > 255) {
    return { valid: false, error: 'Name is too long (max 255 characters)' };
  }

  // Reserved names on Windows (block for cross-platform safety)
  const reservedNames = [
    'CON', 'PRN', 'AUX', 'NUL',
    'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8', 'COM9',
    'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9',
  ];
  const nameWithoutExt = trimmed.split('.')[0].toUpperCase();
  if (reservedNames.includes(nameWithoutExt)) {
    return { valid: false, error: 'This name is reserved by the system' };
  }

  return { valid: true };
}

interface InlineNameEditorProps {
  /** Initial value to display in the input */
  initialValue: string;
  /** Called when editing is confirmed (Enter pressed or blur) */
  onConfirm: (newValue: string) => void;
  /** Called when editing is cancelled (Escape pressed) */
  onCancel: () => void;
  /** Whether to select just the name part (without extension) on focus */
  selectNameOnly?: boolean;
  /** Additional class names */
  className?: string;
}

export function InlineNameEditor({
  initialValue,
  onConfirm,
  onCancel,
  selectNameOnly = true,
  className,
}: InlineNameEditorProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [value, setValue] = useState(initialValue);
  const [hasSubmitted, setHasSubmitted] = useState(false);

  // Validate current value
  const validation = useMemo(() => validateFilename(value), [value]);

  // Focus and select on mount
  useEffect(() => {
    const input = inputRef.current;
    if (!input) return;

    input.focus();

    if (selectNameOnly && initialValue.includes('.')) {
      // Select only the name part (before the last dot)
      const lastDotIndex = initialValue.lastIndexOf('.');
      input.setSelectionRange(0, lastDotIndex);
    } else {
      // Select all
      input.select();
    }
  }, [initialValue, selectNameOnly]);

  const handleConfirm = useCallback(() => {
    if (hasSubmitted) return;

    const trimmedValue = value.trim();

    // Check if value is unchanged
    if (!trimmedValue || trimmedValue === initialValue) {
      onCancel();
      return;
    }

    // Validate before confirming
    const result = validateFilename(trimmedValue);
    if (!result.valid) {
      // Don't submit invalid names - shake the input or show error
      const input = inputRef.current;
      if (input) {
        input.classList.add('animate-shake');
        setTimeout(() => input.classList.remove('animate-shake'), 500);
      }
      return;
    }

    setHasSubmitted(true);
    onConfirm(trimmedValue);
  }, [value, initialValue, onConfirm, onCancel, hasSubmitted]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        handleConfirm();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onCancel();
      }
    },
    [handleConfirm, onCancel]
  );

  const handleBlur = useCallback(() => {
    // Small delay to allow click events to fire first
    setTimeout(() => {
      if (!hasSubmitted) {
        handleConfirm();
      }
    }, 100);
  }, [handleConfirm, hasSubmitted]);

  // Prevent clicks from bubbling to parent (which might clear selection)
  const handleClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
  }, []);

  const hasError = !validation.valid && value !== initialValue && value.trim() !== '';

  return (
    <div className="relative inline-block">
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
        onBlur={handleBlur}
        onClick={handleClick}
        onMouseDown={(e) => e.stopPropagation()}
        aria-invalid={hasError}
        aria-describedby={hasError ? 'name-error' : undefined}
        className={cn(
          'bg-white dark:bg-gray-800 border',
          hasError
            ? 'border-red-500 dark:border-red-400'
            : 'border-blue-500 dark:border-blue-400',
          'rounded px-1.5 py-0.5 text-sm outline-none',
          'text-gray-900 dark:text-gray-100',
          hasError
            ? 'focus:ring-2 focus:ring-red-500/30'
            : 'focus:ring-2 focus:ring-blue-500/30',
          className
        )}
        style={{ minWidth: '100px' }}
      />
      {hasError && validation.error && (
        <div
          id="name-error"
          role="alert"
          className="absolute left-0 top-full mt-1 px-2 py-1 text-xs text-white bg-red-500 rounded shadow-lg whitespace-nowrap z-50"
        >
          {validation.error}
        </div>
      )}
    </div>
  );
}
