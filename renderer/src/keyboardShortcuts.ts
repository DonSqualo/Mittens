export type ShortcutAction = 'open-graph-menu' | 'reset-camera' | null;

export interface ShortcutEvent {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  repeat: boolean;
}

export function get_shortcut_action(event: ShortcutEvent, typingContext: boolean): ShortcutAction {
  if (typingContext) return null;

  const key = event.key.toLowerCase();

  if ((event.ctrlKey || event.metaKey) && key === 'k') {
    return 'open-graph-menu';
  }

  // Ignore held key repeats so "h" only resets once per press.
  if (!event.repeat && !event.ctrlKey && !event.metaKey && key === 'h') {
    return 'reset-camera';
  }

  return null;
}
