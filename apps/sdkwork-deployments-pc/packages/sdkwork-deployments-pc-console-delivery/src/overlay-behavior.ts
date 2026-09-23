import { useEffect, useRef, type RefObject } from "react";

/**
 * Whether `panel` is the innermost open dialog.
 *
 * Read off the DOM rather than kept in React state, because the answer is a
 * property of what is *mounted*, and every overlay on this page — the drawer and
 * the dialogs it opens over itself — is mounted and unmounted by its owner. A
 * registry threaded through context would answer the same question with one more
 * thing to keep in sync.
 *
 * `querySelectorAll` returns document order, so the last panel is the one that was
 * rendered deepest: the picker opened from inside the wizard sits inside the
 * wizard's own panel and therefore comes last.
 */
export function isInnermostDialog(panel: HTMLElement | null): boolean {
  if (panel === null) return false;
  const dialogs = document.querySelectorAll('[role="dialog"], [role="alertdialog"]');
  return dialogs.length > 0 && dialogs[dialogs.length - 1] === panel;
}

/**
 * Escape closes the panel this hook is mounted by — and only when that panel is
 * the innermost one, so a picker opened over the wizard closes by itself.
 *
 * `close` is held in a ref rather than declared as a dependency: every caller
 * passes a fresh arrow function on each render, and re-subscribing the listener
 * per keystroke's worth of state is churn nothing here needs.
 */
export function useDialogEscape(panelRef: RefObject<HTMLElement | null>, close: () => void): void {
  const closeRef = useRef(close);
  useEffect(() => { closeRef.current = close; }, [close]);
  useEffect(() => {
    function onKeyDown(event: globalThis.KeyboardEvent) {
      if (event.key !== "Escape" || !isInnermostDialog(panelRef.current)) return;
      event.preventDefault();
      closeRef.current();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [panelRef]);
}
