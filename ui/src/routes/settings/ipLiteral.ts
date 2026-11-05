// Client-side check for the `api.bind_address` row (`03 §8`, `UI_REFERENCE §4`):
// the backend parses the value as an IPv4/IPv6 literal before it saves it, so an
// invalid literal has to fail here first, visibly, with the typed value kept —
// never a silent clamp and never a save the user has to discover failed.
//
// The rules mirror a standard literal parse: four dotted decimal octets with no
// leading zeros, or hex groups with at most one `::` and an optional dotted quad in
// the last slot. Ports and brackets belong to a client URL, not to this value.
function isIpv4Literal(value: string): boolean {
  const parts = value.split(".");
  return (
    parts.length === 4 &&
    parts.every(
      (part) =>
        /^\d{1,3}$/.test(part) &&
        Number(part) <= 255 &&
        (part.length === 1 || part.charAt(0) !== "0"),
    )
  );
}

function isIpv6Literal(value: string): boolean {
  // A zone id (`fe80::1%eth0`) is not part of an address literal.
  if (value.includes("%")) return false;
  const halves = value.split("::");
  if (halves.length > 2) return false;
  const head = halves[0].length === 0 ? [] : halves[0].split(":");
  const tail =
    halves.length === 2 && halves[1].length > 0 ? halves[1].split(":") : [];
  const groups = [...head, ...tail];
  const last = groups[groups.length - 1];
  const embedded = last !== undefined && last.includes(".");
  if (embedded && !isIpv4Literal(last)) return false;
  const hex = embedded ? groups.slice(0, -1) : groups;
  if (!hex.every((group) => /^[0-9a-f]{1,4}$/i.test(group))) return false;
  const count = hex.length + (embedded ? 2 : 0);
  // `::` stands for at least one zero group, so a compressed form must be short.
  return halves.length === 2 ? count < 8 : count === 8;
}

export function isIpLiteral(value: string): boolean {
  if (value.length === 0) return false;
  return value.includes(":") ? isIpv6Literal(value) : isIpv4Literal(value);
}
