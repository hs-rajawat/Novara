// Deterministic colors derived from strings, used for cover-art fallbacks
// so every game without artwork still gets a stable, distinct tint.

export function hueFromString(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = (h * 31 + s.charCodeAt(i)) % 360;
  }
  return h;
}

export function coverGradient(title: string): string {
  const hue = hueFromString(title);
  const hue2 = (hue + 50) % 360;
  return `linear-gradient(140deg, hsl(${hue} 45% 24%) 0%, hsl(${hue2} 55% 12%) 100%)`;
}
