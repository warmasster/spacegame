// The players as people of the world (docs/MUNDO.md §12): an entity each, found by name (the same
// person every session), so facts about them (who took what, who shot) point at someone the world
// remembers.

import { defineComponent } from '../core/store.js';
import type { SimModule } from '../core/world.js';

export const Player = defineComponent('player', { name: 'sym', sessions: 'u32' });

export const players: SimModule = {
  name: 'players',
  components: [Player],
  requests: {
    /** A player came in: its entity (made the first time a name is seen). */
    'players.join': (w, o: { name: string }) => {
      const name = String(o?.name ?? '').slice(0, 40) || 'anónimo';
      const t = w.table(Player);
      const sym = w.syms.id(name);
      for (let r = 0; r < t.count; r++) {
        if (t.c.name[r] !== sym) continue;
        t.c.sessions[r]++;
        return t.ids[r];
      }
      const id = w.spawn();
      t.add(id, { name: sym, sessions: 1 });
      w.record('player.first', id, 0, 0, { keep: true, data: { name } });
      return id;
    },
  },
  describe: (w, id) => (w.table(Player).has(id) ? { what: 'jugador', name: w.syms.name(w.table(Player).get(id, 'name')) } : null),
  gauges: {
    'players.known': (w) => w.table(Player).count,
  },
};
