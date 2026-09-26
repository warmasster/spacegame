// Manufacturers: every catalog component has one. The maker's trim colour marks its machines on
// board (bands, handles, caps), so two ships built from the same parts can still look their own
// (a ship can repaint a part with `maker` in its placement).

import type { RGB } from '../def.js';

export interface MakerDef {
  id: string;
  name: string;
  /** Trim colour (linear RGB) of its machines. */
  trim: RGB;
}

export const MAKERS: Record<string, MakerDef> = {
  kestrel: { id: 'kestrel', name: 'Kestrel Dinámica', trim: [0.5, 0.2, 0.02] },
  hokuto: { id: 'hokuto', name: 'Hokuto Industrial', trim: [0.03, 0.26, 0.28] },
  vanta: { id: 'vanta', name: 'Vanta Orbital', trim: [0.42, 0.03, 0.04] },
  lunagen: { id: 'lunagen', name: 'Lunagen', trim: [0.55, 0.4, 0.02] },
  selk: { id: 'selk', name: 'Selk & Hijos', trim: [0.16, 0.18, 0.2] },
};

export function maker(id: string | undefined): MakerDef {
  return (id && MAKERS[id]) || MAKERS.selk;
}
