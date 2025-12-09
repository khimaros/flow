import type { SelectOption } from "../types";

const CACHE_PREFIX = "flow:dynamic-options:";

/**
 * Generate a cache key for dynamic select options
 */
function getCacheKey(
  workflowName: string | null,
  nodeId: string,
  inputName: string,
): string {
  const workflow = workflowName || "__unsaved__";
  return `${CACHE_PREFIX}${workflow}:${nodeId}:${inputName}`;
}

/**
 * Get cached options from localStorage
 */
export function getCachedOptions(
  workflowName: string | null,
  nodeId: string,
  inputName: string,
): SelectOption[] {
  const key = getCacheKey(workflowName, nodeId, inputName);
  try {
    const cached = localStorage.getItem(key);
    if (cached) {
      return JSON.parse(cached);
    }
  } catch (e) {
    console.warn("Failed to read cached options:", e);
  }
  return [];
}

/**
 * Save options to localStorage cache
 */
export function setCachedOptions(
  workflowName: string | null,
  nodeId: string,
  inputName: string,
  options: SelectOption[],
): void {
  const key = getCacheKey(workflowName, nodeId, inputName);
  try {
    localStorage.setItem(key, JSON.stringify(options));
  } catch (e) {
    console.warn("Failed to cache options:", e);
  }
}

/**
 * Clear cached options for a specific input
 */
export function clearCachedOptions(
  workflowName: string | null,
  nodeId: string,
  inputName: string,
): void {
  const key = getCacheKey(workflowName, nodeId, inputName);
  try {
    localStorage.removeItem(key);
  } catch (e) {
    console.warn("Failed to clear cached options:", e);
  }
}

/**
 * Clear all cached options for a workflow
 */
export function clearWorkflowOptionsCache(workflowName: string | null): void {
  const workflow = workflowName || "__unsaved__";
  const prefix = `${CACHE_PREFIX}${workflow}:`;
  try {
    const keysToRemove: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key?.startsWith(prefix)) {
        keysToRemove.push(key);
      }
    }
    keysToRemove.forEach((key) => localStorage.removeItem(key));
  } catch (e) {
    console.warn("Failed to clear workflow options cache:", e);
  }
}
