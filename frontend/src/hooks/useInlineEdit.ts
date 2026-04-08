import { useState, useCallback } from 'react';

interface UseInlineEditOptions<T> {
  /** Current value from parent */
  value: T;
  /** Callback when value is submitted */
  onSubmit: (value: T) => void;
  /** Validate and parse input string to value. Return null if invalid. */
  parse: (input: string) => T | null;
  /** Convert value to string for display */
  stringify?: (value: T) => string;
}

interface UseInlineEditResult {
  /** Whether currently in edit mode */
  isEditing: boolean;
  /** Current input value as string */
  inputValue: string;
  /** Start editing mode */
  startEdit: () => void;
  /** Handle input change */
  handleInputChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  /** Submit current input (validates first) */
  handleSubmit: () => void;
  /** Handle keyboard events (Enter to submit, Escape to cancel) */
  handleKeyDown: (e: React.KeyboardEvent) => void;
}

/**
 * Hook for inline editing of a value.
 * Handles edit mode toggle, validation, and keyboard shortcuts.
 */
export function useInlineEdit<T>({
  value,
  onSubmit,
  parse,
  stringify = String,
}: UseInlineEditOptions<T>): UseInlineEditResult {
  const [isEditing, setIsEditing] = useState(false);
  const [inputValue, setInputValue] = useState(stringify(value));

  const startEdit = useCallback(() => {
    setIsEditing(true);
    setInputValue(stringify(value));
  }, [stringify, value]);

  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    setInputValue(e.target.value);
  }, []);

  const handleSubmit = useCallback(() => {
    const parsed = parse(inputValue);
    if (parsed !== null) {
      onSubmit(parsed);
    } else {
      // Revert to original value on invalid input
      setInputValue(stringify(value));
    }
    setIsEditing(false);
  }, [inputValue, onSubmit, parse, stringify, value]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleSubmit();
    } else if (e.key === 'Escape') {
      setInputValue(stringify(value));
      setIsEditing(false);
    }
  }, [handleSubmit, stringify, value]);

  return {
    isEditing,
    inputValue,
    startEdit,
    handleInputChange,
    handleSubmit,
    handleKeyDown,
  };
}
