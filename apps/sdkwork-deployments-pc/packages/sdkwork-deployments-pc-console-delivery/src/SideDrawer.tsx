import { X } from "lucide-react";
import { useEffect, useId, useRef, type ReactNode } from "react";

import { useDialogEscape } from "./overlay-behavior.ts";

export interface SideDrawerProps {
  /** The scrolling region. Everything the operator reads and fills in goes here. */
  children: ReactNode;
  close(): void;
  closeLabel: string;
  /**
   * The pinned action row.
   *
   * Rendered as the panel's last flex row rather than as the last thing in
   * `children`, which is the whole point of the component: a submit row placed
   * inside the scroller scrolls away from the button that produced it, so a form
   * answered top to bottom ends with its own confirmation already off screen.
   *
   * Left `undefined` for a drawer that only reads (no action row is drawn at all,
   * rather than an empty bar with a border).
   */
  footer?: ReactNode;
  /**
   * Which control the drawer opens on.
   *
   * `true` (the default) focuses the panel itself: the drawer opens on a summary
   * the operator is meant to read, and landing the caret in a text field picks a
   * field for them. `false` is for the drawers whose first control *is* the task —
   * a create form whose only required field carries `autoFocus` — where taking the
   * caret back would undo React's own focus and put the operator one field past the
   * one they are there to fill.
   *
   * Read once per mount (see below) rather than tracked reactively.
   */
  focusPanel?: boolean;
  title: string;
}

/**
 * The side panel a console form is filled in on, as three slots.
 *
 * `header` (title + close) and `footer` (the action row) are `flex: 0 0 auto` and
 * the body between them is the only thing that scrolls, so a form taller than the
 * window spends its height in the middle of the panel: the title and the submit row
 * stay where the operator last saw them. That is the whole contract, and it is why
 * the footer is a prop rather than a child — the three bands have to be siblings for
 * the flex column to pin the outer two.
 *
 * ## Why this is not the design system's `Drawer`
 *
 * `@sdkwork/ui-pc-react` exports a `Drawer` with exactly this header/body/footer
 * contract, and it is the right long-term home for this one. It is not used here
 * yet because `DrawerContent` portals unconditionally to `document.body`, and every
 * rule that styles this surface — the form grid, the fieldsets, the hostname picker
 * the wizard opens over itself — is scoped under the host's `.deploy-surface`
 * element. A portalled panel leaves that element, so the drawer and everything in it
 * would come out unstyled in the Web Server console (104 scoped rules), and the
 * library reads its portal container from nowhere: it forwards no `container` prop.
 *
 * So the DOM below is deliberate in two ways, and both are what a migration to the
 * design system would replace:
 *
 * 1. It renders in place. The panel is a child of the page that opened it, which is
 *    what keeps it inside the scope its stylesheet was written against.
 * 2. It carries `dialog` and `delivery-dialog` as well as `delivery-drawer`. The
 *    first two are where this surface's density pass hangs — borderless fieldsets,
 *    the one-field-per-row parameter grid, the label voice, the footer's own
 *    right-aligned flex row. A drawer that re-declared them by hand would be the
 *    same form at a second, drifting set of measurements.
 *
 * The props above are the interface a Radix-based implementation would also satisfy,
 * so swapping the body of this function is the whole migration.
 */
export function SideDrawer({ children, close, closeLabel, focusPanel = true, footer, title }: SideDrawerProps) {
  const titleId = useId();
  const panelRef = useRef<HTMLElement | null>(null);
  // Read once, into a value the effect below does not depend on: which control the
  // drawer opens on is settled when it mounts, and making it reactive would re-run a
  // mount-scoped focus effect on every render of the form inside.
  const focusPanelOnMount = useRef(focusPanel).current;
  useDialogEscape(panelRef, close);
  useEffect(() => {
    const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    // `preventScroll` keeps the browser from scrolling the page behind the drawer to
    // bring the panel into view.
    if (focusPanelOnMount) panelRef.current?.focus({ preventScroll: true });
    // The backdrop blocks the pointer but not the wheel, so a ledger taller than the
    // window would still scroll underneath the open drawer.
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
      previouslyFocused?.focus();
    };
    // Mount-scoped on purpose: the drawer is mounted and unmounted by its owner, and
    // re-running this on every render of the wizard would steal focus back from
    // whatever field the operator had just reached.
  }, []);
  return <div className="delivery-drawer-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) close(); }}>
    <aside
      ref={panelRef}
      className="dialog delivery-dialog delivery-drawer"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      tabIndex={-1}
    >
      <header><h2 id={titleId}>{title}</h2><button className="icon-button" type="button" title={closeLabel} onClick={close}><X size={18} /></button></header>
      <div className="delivery-drawer-body">{children}</div>
      {footer !== undefined && <div className="delivery-drawer-footer">{footer}</div>}
    </aside>
  </div>;
}
