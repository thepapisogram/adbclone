import type { Safety } from "../lib/types";

const text: Record<Safety, string> = { safe: "safe", caution: "caution", unsafe: "unsafe", unknown: "not rated" };
const hint: Record<Safety, string> = {
  safe: "Generally safe and reversible",
  caution: "May affect features you use; check before removing",
  unsafe: "Can break the phone or boot; do not remove",
  unknown: "No rating for this package; not implied safe",
};

/** `kind` says which action the tag is about, e.g. "Disable: safe". */
export function SafetyTag({ kind, value }: { kind: "Disable" | "Uninstall"; value: Safety }) {
  return (
    <span className={`pill safety-${value}`} title={`${kind}: ${hint[value]}`}>
      {text[value]}
    </span>
  );
}
