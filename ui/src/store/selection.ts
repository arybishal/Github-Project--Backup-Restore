import { useSyncExternalStore } from 'react';

/**
 * Module-level selection store: the Repositories page writes the selected
 * repository full names here and the Backup page reads them. Also carries the
 * preselected folder for the GitHub Publisher page.
 */

let selectedRepos: string[] = [];
const listeners = new Set<() => void>();

function emit(): void {
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function setSelectedRepos(next: string[]): void {
  selectedRepos = next;
  emit();
}

export function getSelectedRepos(): string[] {
  return selectedRepos;
}

export function useSelectedRepos(): string[] {
  return useSyncExternalStore(subscribe, getSelectedRepos);
}

let publishPath: string | null = null;

export function setPublishPath(path: string | null): void {
  publishPath = path;
}

export function getPublishPath(): string | null {
  return publishPath;
}
