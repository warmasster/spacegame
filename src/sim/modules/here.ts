// "What is here" (docs/MUNDO.md §12): the engine asks the world what there is round a point — each
// module says what its entities are (SimModule.describe); the answer is plain data, nearest first.

import type { SimModule } from '../core/world.js';
import { near, Place, placeOf, type V3 } from '../kit/place.js';

export interface HereItem {
  id: number;
  /** Distance (m). */
  d: number;
  [k: string]: unknown;
}

export const here: SimModule = {
  name: 'here',
  components: [Place],
  requests: {
    /** Everything within `r` m of point `p` of host `host`'s space (0: the world). */
    'world.here': (w, o: { host?: number; p: V3; r: number; limit?: number }) => {
      const host = o.host ?? 0;
      const out: HereItem[] = [];
      for (const id of near(w, host, o.p, o.r)) {
        let what: Record<string, unknown> | null = null;
        for (const m of w.modules) {
          what = m.describe?.(w, id) ?? null;
          if (what) break;
        }
        if (!what) continue;
        const at = placeOf(w, id)!;
        out.push({ id, d: Math.hypot(at.p[0] - o.p[0], at.p[1] - o.p[1], at.p[2] - o.p[2]), ...what });
      }
      return out.sort((a, b) => a.d - b.d || a.id - b.id).slice(0, o.limit ?? 50);
    },
  },
};
