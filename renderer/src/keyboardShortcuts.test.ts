import test from 'node:test';
import assert from 'node:assert/strict';

import { get_shortcut_action } from './keyboardShortcuts.ts';

test('returns null while typing in an input context', () => {
  const action = get_shortcut_action({ key: 'h', ctrlKey: false, metaKey: false, repeat: false }, true);
  assert.equal(action, null);
});

test('maps Ctrl/Cmd+K to graph menu action', () => {
  assert.equal(
    get_shortcut_action({ key: 'k', ctrlKey: true, metaKey: false, repeat: false }, false),
    'open-graph-menu'
  );
  assert.equal(
    get_shortcut_action({ key: 'K', ctrlKey: false, metaKey: true, repeat: false }, false),
    'open-graph-menu'
  );
});

test('maps single H press to reset camera', () => {
  const action = get_shortcut_action({ key: 'H', ctrlKey: false, metaKey: false, repeat: false }, false);
  assert.equal(action, 'reset-camera');
});

test('ignores repeated H keydown events', () => {
  const action = get_shortcut_action({ key: 'h', ctrlKey: false, metaKey: false, repeat: true }, false);
  assert.equal(action, null);
});

test('ignores modified H shortcuts', () => {
  assert.equal(
    get_shortcut_action({ key: 'h', ctrlKey: true, metaKey: false, repeat: false }, false),
    null
  );
  assert.equal(
    get_shortcut_action({ key: 'h', ctrlKey: false, metaKey: true, repeat: false }, false),
    null
  );
});
