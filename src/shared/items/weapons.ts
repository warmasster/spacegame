// Weapon and tool catalog: what the crew carries and what is bolted to things. A hand-held entry is
// equipable (number keys in catalog order, holstered on the suit's pack when put away); a mounted
// one (`mounted`) is fired by a mount (shared/items/mounts.ts: a turret, a fixed gun) on a
// ship, a base, anything. Each does one thing with the trigger: fire a projectile of the projectile
// catalog, or work on a ship (the welder). The server checks shots against these numbers; the
// client adds how a hand-held one looks and is held (client/fx/weapons.ts).

import { PROJECTILES } from './projectiles.js';

/** What the trigger does. */
export type WeaponAction =
  /** Fire a projectile (catalog id) along the aim; `auto`: keeps firing while the trigger is held. */
  | { kind: 'fire'; projectile: string; auto?: boolean }
  /** Weld and repair ship panels and machines while held (client/ship/interaction.ts). */
  | { kind: 'weld' };

export interface WeaponDef {
  id: string;
  name: string;
  action: WeaponAction;
  /** Fired only by a mount (shared/items/mounts.ts), never carried: no number key, no look in the hands. */
  mounted?: boolean;
  /** Seconds between shots. */
  cooldown: number;
  /** Momentum given to the shooter per shot (N·s): body, arm and camera recoil. */
  recoil: number;
  /** Helmet display: ready to fire / working. */
  hud: { ready: string; busy?: string };
  /** Sound bank ids. */
  sounds?: { fire?: string };
}

/** Hand-held ones in catalog order: the number keys pick them (1, 2, 3…). */
export const WEAPON_LIST: WeaponDef[] = [];
export const WEAPON_DEFS: Record<string, WeaponDef> = {};

/** A weapon by id (anything from the network: only the catalog's own entries). */
export const weaponById = (id: unknown): WeaponDef | undefined => (typeof id === 'string' && Object.hasOwn(WEAPON_DEFS, id) ? WEAPON_DEFS[id] : undefined);

export function defineWeapon(def: WeaponDef): WeaponDef {
  if (WEAPON_DEFS[def.id]) throw new Error(`weapon "${def.id}" defined twice`);
  if (def.action.kind === 'fire' && !PROJECTILES[def.action.projectile]) throw new Error(`weapon "${def.id}": unknown projectile "${def.action.projectile}"`);
  WEAPON_DEFS[def.id] = def;
  if (!def.mounted) WEAPON_LIST.push(def);
  return def;
}

/** The projectile a weapon fires (null: it fires nothing). */
export function projectileOf(def: WeaponDef | undefined) {
  return def?.action.kind === 'fire' ? PROJECTILES[def.action.projectile] : null;
}

defineWeapon({
  id: 'launcher',
  name: 'Lanzacohetes',
  action: { kind: 'fire', projectile: 'rocket' },
  cooldown: 1.2,
  recoil: 70,
  hud: { ready: 'COHETE LISTO' },
  sounds: { fire: 'launcher.fire' },
});

defineWeapon({
  id: 'welder',
  name: 'Soldadora',
  action: { kind: 'weld' },
  cooldown: 0,
  recoil: 0,
  hud: { ready: 'SOLDADORA LISTA', busy: 'SOLDANDO' },
});

// a light carbine: short bursts while the trigger is held
defineWeapon({
  id: 'rifle',
  name: 'Fusil',
  action: { kind: 'fire', projectile: 'bullet', auto: true },
  cooldown: 0.16,
  recoil: 7,
  hud: { ready: 'FUSIL LISTO' },
  sounds: { fire: 'rifle.fire' },
});

// the tube of a mini-missile turret (shared/items/mounts.ts): only a mount fires it
defineWeapon({
  id: 'minimissile.tube',
  name: 'Tubo de minimisiles',
  mounted: true,
  action: { kind: 'fire', projectile: 'minimissile' },
  cooldown: 0.45,
  recoil: 0,
  hud: { ready: 'MINIMISIL LISTO' },
  sounds: { fire: 'turret.fire' },
});
