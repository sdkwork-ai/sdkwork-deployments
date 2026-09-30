/**
 * The ledger's identity tile: gradient, glyph, and — when the app has one —
 * the uploaded store icon fading in over it.
 *
 * Layering is the polish: the gradient tile renders **immediately** from row
 * data (see `service/app-icon.ts` for why each layer is deterministic), so a
 * hundred-row ledger never shows a blank box or a spinner where an identity
 * belongs. The signed-URL exchange then runs in the background and the real
 * icon crossfades in only when the bitmap has actually decoded — an `<img>`
 * that pops in half-loaded shows a broken glyph for a frame, and a fallback
 * that swaps on `load` *event* alone still flashes on slow networks. Until
 * `onload` fires, the tile simply stays; on error (denied, deleted node,
 * signature trouble) it stays permanently, which is the design, not a defect.
 *
 * The component is purely presentational: it never sees a client — the page
 * injects the resolver function (`createAppIconUrlResolver`) — and it renders
 * `aria-hidden`, because the identity it conveys (name, type) is already the
 * row's text content and the 型别 column; an accessible name here would make
 * screen readers say every app twice.
 */
import { useEffect, useState } from "react";
import { appAvatarGradientClass, appAvatarIconKey, readAppIconDriveNodeId } from "../service/app-icon.ts";
import { DeployAppTypeIcon } from "./DeployAppTypeIcon.tsx";

export interface AppAvatarProps {
  /** The row's avatar inputs: slug seeds the palette, kind (and surface) pick the glyph. */
  readonly app: Parameters<typeof appAvatarIconKey>[0]
  /**
   * Injected URL resolver over `deploy_app.metadata.media.icon`. Optional so a
   * host that cannot reach Drive (or a test) still gets the full fallback tile;
   * when present but the exchange fails, the tile stays — silently by design.
   */
  readonly resolveIconUrl?: ((nodeId: string, signal: AbortSignal) => Promise<string | undefined>) | undefined
}

/** @returns the 32px identity tile for one application row. */
export function AppAvatar({ app, resolveIconUrl }: AppAvatarProps) {
  const iconNodeId = readAppIconDriveNodeId(app.metadata)
  const [iconUrl, setIconUrl] = useState<string>()
  const [iconFailed, setIconFailed] = useState(false)
  const [iconLoaded, setIconLoaded] = useState(false)

  useEffect(() => {
    // Re-resolve per (app, resolver) pair, and reset the bitmap state when the
    // row is reused for a different app — a stale icon on a recycled row is
    // worse than no icon, because it is *confidently* wrong.
    setIconUrl(undefined)
    setIconFailed(false)
    setIconLoaded(false)
    if (iconNodeId === undefined || resolveIconUrl === undefined) return
    const controller = new AbortController()
    void resolveIconUrl(iconNodeId, controller.signal).then((url) => {
      if (controller.signal.aborted) return
      if (url === undefined) setIconFailed(true)
      else setIconUrl(url)
    })
    return () => { controller.abort() }
  }, [iconNodeId, resolveIconUrl])

  const showImage = iconUrl !== undefined && !iconFailed
  return (
    <span
      aria-hidden="true"
      className={`apps-avatar ${appAvatarGradientClass(app.slug)}${showImage ? " apps-avatar-with-image" : ""}`}
    >
      <DeployAppTypeIcon className="apps-avatar-glyph" iconKey={appAvatarIconKey(app)} size={18} />
      {showImage && (
        <img
          alt=""
          className={`apps-avatar-image${iconLoaded ? " apps-avatar-image-loaded" : ""}`}
          decoding="async"
          loading="lazy"
          onError={() => { setIconFailed(true) }}
          onLoad={() => { setIconLoaded(true) }}
          src={iconUrl}
        />
      )}
    </span>
  )
}
