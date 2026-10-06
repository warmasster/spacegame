import type * as THREE from 'three';

/** Shared visible/depth displacement. Auxiliary views change uViewer without changing geometry. */
export const MORPH_GLSL = /* glsl */ `
  vec3 cdlodWorld = (modelMatrix * vec4(transformed, 1.0)).xyz;
  float cdlodK = clamp((distance(cdlodWorld, uViewer) - morph.w * 0.68) / (morph.w * 0.3), 0.0, 1.0);
  #ifdef DBG_NOMORPH
  cdlodK = 0.0;
  #endif
  transformed = mix(transformed, morph.xyz, cdlodK);
`;

/** Skirts close transient cracks but are not terrain: they must never cast a border's shadow. */
export function patchTerrainDepth(shader: { vertexShader: string; fragmentShader: string; uniforms: Record<string, THREE.IUniform> }, viewer: THREE.IUniform, gridVertices: number) {
  shader.uniforms.uViewer = viewer;
  shader.vertexShader = shader.vertexShader
    .replace('#include <common>', '#include <common>\nattribute vec4 morph;\nuniform vec3 uViewer;\nvarying float vSkirtDepth;')
    .replace('#include <begin_vertex>', `#include <begin_vertex>\n${MORPH_GLSL}\nvSkirtDepth = float(gl_VertexID >= ${gridVertices});`);
  shader.fragmentShader = shader.fragmentShader
    .replace('#include <common>', '#include <common>\nvarying float vSkirtDepth;')
    .replace('#include <clipping_planes_fragment>', '#include <clipping_planes_fragment>\nif (vSkirtDepth > 0.0) discard;');
}
