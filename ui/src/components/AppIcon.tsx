import { useIcon } from "../lib/icons";

/** The app's real icon when known, else a neutral placeholder of the same size. */
export function AppIcon({ name, size = 24 }: { name: string; size?: number }) {
  const src = useIcon(name);
  return src
    ? <img className="appicon" src={src} width={size} height={size} alt="" loading="lazy" />
    : <span className="appicon ph" style={{ width: size, height: size }} aria-hidden="true" />;
}
