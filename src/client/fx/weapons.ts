import * as THREE from 'three';

/**
 * Weapon definitions. Every weapon is data + a mesh builder with named sockets in its local
 * space (+Z forward, origin = the point that rests on the carrier: shoulder or hip). The
 * astronaut rig places the weapon from `carry`, and the hands IK onto the grip sockets, so new
 * weapons need no animation work.
 */
/** A graspable handle: cylinder centre, handle axis (unit) and radius, in weapon space. */
export interface Grip {
  pos: THREE.Vector3;
  axis: THREE.Vector3;
  radius: number;
  /** Which side the hand approaches from (weapon space), e.g. -X for the right hand. */
  side: THREE.Vector3;
}

export interface WeaponDef {
  id: string;
  name: string;
  carry: 'shoulder' | 'hip';
  /** Handles for the right (trigger) hand and the left (support) hand. */
  rightGrip: Grip;
  leftGrip: Grip;
  muzzle: THREE.Vector3;
  /** Where it hangs when holstered (model space offset + euler), on the PLSS. */
  holster: { pos: THREE.Vector3; rot: THREE.Euler };
  /** Momentum given to the shooter per shot (N·s): drives body, arm and camera recoil. */
  recoil: number;
  cooldown: number;
  build(): THREE.Object3D;
}

function lathe(points: Array<[number, number]>, segs = 28) {
  const g = new THREE.LatheGeometry(points.map(([r, y]) => new THREE.Vector2(r, y)), segs);
  g.rotateX(Math.PI / 2); // lathe axis Y → weapon axis Z
  g.computeVertexNormals();
  return g;
}

function part(geo: THREE.BufferGeometry, mat: THREE.Material, x = 0, y = 0, z = 0, rx = 0) {
  const m = new THREE.Mesh(geo, mat);
  m.position.set(x, y, z);
  m.rotation.x = rx;
  m.castShadow = true;
  return m;
}

/** M9-style rocket launcher: long tube over the shoulder, pistol grip + fore-grip below. */
export const LAUNCHER: WeaponDef = {
  id: 'launcher',
  name: 'Lanzacohetes',
  carry: 'shoulder',
  // pistol grip raked back 0.25 rad, fore-grip raked forward 0.15 rad (see build())
  rightGrip: { pos: new THREE.Vector3(0, -0.075, 0.2), axis: new THREE.Vector3(0, -Math.cos(0.25), -Math.sin(0.25)), radius: 0.02, side: new THREE.Vector3(-1, 0, 0) },
  leftGrip: { pos: new THREE.Vector3(0, -0.05, 0.4), axis: new THREE.Vector3(0, -Math.cos(0.15), Math.sin(0.15)), radius: 0.02, side: new THREE.Vector3(1, 0, 0) },
  muzzle: new THREE.Vector3(0, 0.065, 0.86),
  holster: { pos: new THREE.Vector3(0.03, 1.36, -0.47), rot: new THREE.Euler(-1.35, 0, 0.62) },
  recoil: 70,
  cooldown: 1.2,
  build() {
    const olive = new THREE.MeshStandardMaterial({ color: 0x4d5243, roughness: 0.55, metalness: 0.35 });
    const dark = new THREE.MeshStandardMaterial({ color: 0x1b1c1e, roughness: 0.45, metalness: 0.7 });
    const g = new THREE.Group();
    const ax = 0.065; // tube axis height above the shoulder contact
    // tube: rear flare behind the shoulder, body, muzzle ring
    g.add(part(lathe([[0.07, -0.62], [0.078, -0.6], [0.06, -0.52], [0.05, -0.5], [0.05, 0.8], [0.058, 0.82], [0.06, 0.87], [0.05, 0.88]]), olive, 0, ax, 0));
    const inner = part(new THREE.CylinderGeometry(0.046, 0.046, 1.48, 20, 1, true).rotateX(Math.PI / 2), new THREE.MeshStandardMaterial({ color: 0x0a0a0a, side: THREE.BackSide }), 0, ax, 0.13);
    g.add(inner);
    // shoulder pad and clamp rings
    g.add(part(new THREE.BoxGeometry(0.07, 0.03, 0.2), dark, 0, 0.012, -0.02));
    for (const z of [-0.3, 0.08, 0.55]) g.add(part(lathe([[0.053, -0.018], [0.058, -0.012], [0.058, 0.012], [0.053, 0.018]]), dark, 0, ax, z));
    // pistol grip + trigger guard, fore-grip, sight
    g.add(part(new THREE.BoxGeometry(0.032, 0.11, 0.045), dark, 0, -0.07, 0.2, 0.25));
    g.add(part(new THREE.BoxGeometry(0.012, 0.04, 0.07), dark, 0, 0.0, 0.16));
    g.add(part(new THREE.BoxGeometry(0.03, 0.1, 0.04), dark, 0, -0.045, 0.4, -0.15));
    g.add(part(new THREE.BoxGeometry(0.03, 0.05, 0.08), dark, -0.06, ax + 0.045, 0.2));
    g.add(part(new THREE.BoxGeometry(0.004, 0.035, 0.004), dark, -0.06, ax + 0.08, 0.72));
    return g;
  },
};

/**
 * Plasma welder / repair tool: a short shoulder-braced gun with a gas bottle. Same handle layout
 * as the launcher, so the same arm IK holds it; slung on the left side of the PLSS when stowed.
 * Fires nothing: held trigger + aim at a hull panel = weld (see ship/interaction.ts).
 */
export const WELDER: WeaponDef = {
  id: 'welder',
  name: 'Soldadora',
  carry: 'shoulder',
  rightGrip: LAUNCHER.rightGrip,
  leftGrip: LAUNCHER.leftGrip,
  muzzle: new THREE.Vector3(0, 0.05, 0.66),
  holster: { pos: new THREE.Vector3(0.34, 1.05, -0.36), rot: new THREE.Euler(-1.2, 0.2, 0.25) },
  recoil: 0,
  cooldown: 0,
  build() {
    const yellow = new THREE.MeshStandardMaterial({ color: 0xc9951c, roughness: 0.5, metalness: 0.25 });
    const dark = new THREE.MeshStandardMaterial({ color: 0x1b1c1e, roughness: 0.45, metalness: 0.7 });
    const copper = new THREE.MeshStandardMaterial({ color: 0xb86a3a, roughness: 0.3, metalness: 1 });
    const g = new THREE.Group();
    const ax = 0.05;
    // body: squat housing over the shoulder, cooling fins, barrel, copper tip
    g.add(part(new THREE.BoxGeometry(0.1, 0.09, 0.42), yellow, 0, ax, 0.02));
    for (let i = 0; i < 5; i++) g.add(part(new THREE.BoxGeometry(0.112, 0.006, 0.05), dark, 0, ax + 0.03, -0.1 + i * 0.045));
    g.add(part(lathe([[0.03, 0.2], [0.032, 0.22], [0.026, 0.5], [0.02, 0.58], [0.014, 0.64]]), dark, 0, ax, 0));
    g.add(part(lathe([[0.014, 0.6], [0.011, 0.66], [0.004, 0.67]]), copper, 0, ax, 0));
    // gas bottle under the body, hose
    g.add(part(lathe([[0, -0.2], [0.03, -0.19], [0.035, -0.15], [0.035, 0.06], [0.02, 0.1], [0, 0.11]]), dark, 0.075, 0.0, -0.05));
    g.add(part(new THREE.TorusGeometry(0.05, 0.006, 6, 12, Math.PI), dark, 0.05, 0.02, 0.12, Math.PI / 2));
    // shoulder pad, pistol grip + guard, fore-grip (same places as the launcher's)
    g.add(part(new THREE.BoxGeometry(0.07, 0.03, 0.18), dark, 0, 0.0, -0.1));
    g.add(part(new THREE.BoxGeometry(0.032, 0.11, 0.045), dark, 0, -0.07, 0.2, 0.25));
    g.add(part(new THREE.BoxGeometry(0.012, 0.04, 0.07), dark, 0, -0.005, 0.16));
    g.add(part(new THREE.BoxGeometry(0.03, 0.1, 0.04), dark, 0, -0.045, 0.4, -0.15));
    g.add(part(new THREE.BoxGeometry(0.03, 0.02, 0.2), yellow, 0, 0.0, 0.34));
    return g;
  },
};

export const WEAPONS: Record<string, WeaponDef> = { launcher: LAUNCHER, welder: WELDER };
