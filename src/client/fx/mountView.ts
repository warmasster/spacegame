import * as THREE from 'three';
import { mountKindById, type MountAim, type MountKindDef, type MountPlacement } from '../../shared/items';
import { Kit, type Slot } from '../ship/models/kit';
import { MOUNT_LOOKS } from './mountLooks';

/** How fast a drawn head catches up with the replicated angles (1/s): the state comes in steps. */
const FOLLOW = 14;

const _l = new THREE.Vector3();
const _u = new THREE.Vector3();
const _f = new THREE.Vector3();
const _m = new THREE.Matrix4();

function meshes(k: Kit, parent: THREE.Object3D, mat: (slot: Slot) => THREE.Material) {
  for (const [slot, geo] of k.build()) {
    const m = new THREE.Mesh(geo, mat(slot));
    m.castShadow = m.receiveShadow = true;
    parent.add(m);
  }
}

/**
 * One weapon mount as drawn, on any host: a group at its placement (host space; hang it from the
 * host's root), the turntable turning with the yaw and the cradle with the pitch, following the
 * angles it is given smoothly (they arrive in steps). `lead`: a gunner on this client, drawn where
 * they aim it now rather than where the authority last said.
 */
export class MountView {
  readonly group = new THREE.Group();
  readonly kind: MountKindDef;
  private yawNode = new THREE.Group();
  private pitchNode = new THREE.Group();
  private drawn: MountAim = { yaw: 0, pitch: 0 };
  private first = true;

  constructor(
    readonly place: MountPlacement,
    mat: (slot: Slot) => THREE.Material,
  ) {
    const kind = mountKindById(place.kind);
    const look = kind && MOUNT_LOOKS[kind.look];
    if (!kind || !look) throw new Error(`mount ${place.id}: kind "${place.kind}" has no look (client/fx/mountLooks.ts)`);
    this.kind = kind;
    this.group.name = `mount:${place.id}`;
    // head space: +X its left (up × forward), +Y its up, +Z its forward
    _u.set(...place.up);
    _f.set(...place.fwd);
    _l.crossVectors(_u, _f).normalize();
    this.group.quaternion.setFromRotationMatrix(_m.makeBasis(_l, _u, _f));
    this.group.position.set(...place.at);
    this.yawNode.position.y = kind.pivot;
    this.group.add(this.yawNode);
    this.yawNode.add(this.pitchNode);
    const tt = new Kit();
    look.turntable(tt);
    meshes(tt, this.yawNode, mat);
    const cr = new Kit();
    look.cradle(cr);
    meshes(cr, this.pitchNode, mat);
  }

  /** Once per frame: the head toward `aim` (a gunner's own lead, or the replicated angles). */
  update(dt: number, aim: MountAim, lead = false) {
    const d = this.drawn;
    if (lead || this.first) {
      d.yaw = aim.yaw;
      d.pitch = aim.pitch;
      this.first = false;
    } else {
      const k = 1 - Math.exp(-FOLLOW * dt);
      let dy = aim.yaw - d.yaw;
      if (!this.kind.yaw) dy = Math.atan2(Math.sin(dy), Math.cos(dy));
      d.yaw += dy * k;
      d.pitch += (aim.pitch - d.pitch) * k;
    }
    this.yawNode.rotation.y = d.yaw;
    this.pitchNode.rotation.x = -d.pitch;
  }
}
