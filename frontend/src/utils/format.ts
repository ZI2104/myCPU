/**
 * Shared formatting utilities for hex values.
 */

/**
 * Format a number as hexadecimal with optional digit padding.
 * @param value - The number to format
 * @param digits - Number of hex digits (default: 8)
 * @returns Hex string with 0x prefix
 */
export function formatHex(value: number | undefined | null, digits: number = 8): string {
  if (value === undefined || value === null) return '0x' + '0'.padStart(digits, '0');
  return '0x' + (value >>> 0).toString(16).toUpperCase().padStart(digits, '0');
}

/**
 * Format a number as short hexadecimal (4 digits, no padding for small values).
 * Used for pipeline stage display where space is limited.
 * @param value - The number to format
 * @returns Short hex string like '0xABCD' or '----' for undefined
 */
export function formatShortHex(value: number | undefined | null): string {
  if (value === undefined || value === null) return '----';
  return '0x' + (value >>> 0).toString(16).toUpperCase().slice(-4);
}

/**
 * Format a number as raw hexadecimal without 0x prefix.
 * Used for memory view where prefix is shown separately.
 * @param value - The number to format
 * @param digits - Number of hex digits
 * @returns Hex string without prefix
 */
export function formatHexRaw(value: number, digits: number): string {
  return value.toString(16).toUpperCase().padStart(digits, '0');
}

/**
 * Format a 64-bit number as 16-digit hexadecimal.
 * @param value - The number to format (may be truncated to 32 bits in JS)
 * @returns 16-digit hex string with 0x prefix
 */
export function formatHex64(value: number | undefined | null): string {
  if (value === undefined || value === null) return '--';
  // JavaScript numbers are 64-bit float, so we can only safely represent 32-bit integers
  // For display purposes, we show as 16-digit hex
  return '0x' + (value >>> 0).toString(16).toUpperCase().padStart(16, '0');
}

/**
 * Format a number as decimal (signed 32-bit).
 * @param value - The number to format
 * @returns Decimal string
 */
export function formatDecimal(value: number): string {
  const signed = value | 0; // Convert to signed 32-bit
  return signed.toString();
}

/**
 * Format a number with locale-specific thousand separators.
 * @param value - The number to format
 * @returns Localized number string
 */
export function formatNumber(n: number): string {
  return Number.isFinite(n) ? n.toLocaleString() : '0';
}
