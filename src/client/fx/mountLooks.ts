import type { Kit } from '../ship/models/kit';

/**
 * How each kind of weapon mount looks (`MountKindDef.look`, shared/items/mounts.ts), in head space
 * with the pivot at the origin: +Z the muzzle, +Y up, +X its left. It is drawn in two pieces that
 * the host's view turns: the turntable turns with the yaw, the cradle (with the barrels) with the
 * yaw and the pitch. The fixed base is the machine the mount rides on (its own model). A new look
 * is a `defineMountLook` here; whatever host carries the mount draws it the same way.
 */
export interface MountLook {
  turntable(k: Kit): void;
  cradle(k: Kit): void;
}

export const MOUNT_LOOKS: Record<string, MountLook> = {};

export function defineMountLook(id: string, look: MountLook) {
  if (MOUNT_LOOKS[id]) throw new Error(`mount look "${id}" defined twice`);
  MOUNT_LOOKS[id] = look;
}

// a squat armoured turntable with a twin-tube cradle between two cheeks
defineMountLook('turret.twin', {
  turntable(k) {
    k.cyl('dark', 'y', 0.36, 0.06, 0, -0.17, 0, 24);
    k.cyl('body', 'y', 0.3, 0.16, 0, -0.08, 0, 20, 0.24);
    for (const s of [-1, 1]) k.box('trim', 0.06, 0.26, 0.34, s * 0.3, 0.02, 0);
    k.box('accent', 0.16, 0.06, 0.12, 0, 0.02, -0.2);
  },
  cradle(k) {
    // the trunnion and the block that holds the tubes
    k.cyl('dark', 'x', 0.05, 0.62, 0, 0, 0, 12);
    k.box('body', 0.46, 0.2, 0.4, 0, 0.02, 0.02);
    for (const s of [-1, 1]) {
      k.cyl('trim', 'z', 0.04, 1.0, s * 0.17, 0.02, 0.05, 14);
      k.cyl('dark', 'z', 0.03, 0.03, s * 0.17, 0.02, 0.555, 14);
      k.torus('chrome', 'z', 0.04, 0.008, s * 0.17, 0.02, 0.35, 14);
    }
    // sensor pod on top
    k.box('dark', 0.1, 0.07, 0.16, 0, 0.16, 0.1);
    k.box('glass', 0.07, 0.04, 0.01, 0, 0.16, 0.185);
  },
});
