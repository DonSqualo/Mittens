import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { Text } from 'troika-three-text';

// Setup
const canvas = document.getElementById('main-canvas') as HTMLCanvasElement;
const container = document.getElementById('main-view')!;

const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
renderer.setPixelRatio(window.devicePixelRatio);
renderer.setClearColor(0x000000, 1);

const scene = new THREE.Scene();

// Camera (Z-up coordinate system)
const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 100000);
camera.up.set(0, 0, 1); // Z is up

// Default camera position (home view)
const DEFAULT_CAMERA = {
  position: new THREE.Vector3(-80, -150, 80),
  target: new THREE.Vector3(0, 0, 20),
  fov: 45
};
camera.position.copy(DEFAULT_CAMERA.position);

// Controls
const controls = new OrbitControls(camera, canvas);
controls.enableDamping = true;
controls.dampingFactor = 0.05;
controls.target.copy(DEFAULT_CAMERA.target);

// Restore camera from localStorage (survives page reloads)
const saved_camera = localStorage.getItem('mittens_camera');
if (saved_camera) {
  try {
    const state = JSON.parse(saved_camera);
    camera.position.set(state.pos[0], state.pos[1], state.pos[2]);
    controls.target.set(state.tgt[0], state.tgt[1], state.tgt[2]);
    if (state.fov) camera.fov = state.fov;
    camera.updateProjectionMatrix();
    controls.update();
    console.log('Restored camera from localStorage');
  } catch (e) {
    console.warn('Failed to restore camera:', e);
  }
}

// Save camera to localStorage on user interaction
controls.addEventListener('end', () => {
  const state = {
    pos: [camera.position.x, camera.position.y, camera.position.z],
    tgt: [controls.target.x, controls.target.y, controls.target.z],
    fov: camera.fov
  };
  localStorage.setItem('mittens_camera', JSON.stringify(state));
});

// Track last Lua camera to avoid resetting on unchanged reloads
let last_lua_camera_key: string | null = null;

// Store home camera position (from Lua, or default)
let home_camera: { pos: [number, number, number], target: [number, number, number], fov: number, near: number, far: number } | null = null;

// Current mesh and field visualizations
let current_mesh: THREE.Mesh | null = null;
let current_edges: THREE.LineSegments | null = null;
let arrows_group: THREE.Group | null = null;
let field_plane: THREE.Mesh | null = null;
let flat_shading: boolean = true;
let show_edges: boolean = false;

// 3D text labels (troika-three-text)
let labels_group: THREE.Group = new THREE.Group();
scene.add(labels_group);

// Measurement markers (GaussMeter, Hydrophone)
let measurement_markers: THREE.Group | null = null;

// Probe line visualizations
let probe_lines: THREE.Group | null = null;

// Circuit overlay element and 3D anchor
const circuit_overlay = document.getElementById('circuit-overlay')!;
const circuit_anchor = new THREE.Vector3(0, 0, 60); // Anchor point above Z axis
let circuit_visible = false;
let circuit_width = 400; // Store circuit width for positioning
const download_stl_btn = document.getElementById('download-stl-btn') as HTMLButtonElement | null;
let current_ws: WebSocket | null = null;
let current_export_files: string[] = [];

// Graph canvas (defined in HTML, hidden by default)
const graph_canvas = document.getElementById('graph-canvas') as HTMLCanvasElement;
const graph_ctx = graph_canvas.getContext('2d')!;
const graph_window = document.getElementById('graph-window')!;

// NanoVNA canvas
const nanovna_canvas = document.getElementById('nanovna-canvas') as HTMLCanvasElement;
const nanovna_ctx = nanovna_canvas.getContext('2d')!;
const nanovna_window = document.getElementById('nanovna-window')!;

declare function openWindow(id: string): void;

// X-ray material (Fresnel-based transparency with vertex colors)
// flat_shading: compute per-face normals for sharp edges
function create_xray_material(flat_shading: boolean = true): THREE.ShaderMaterial {
  const fragment_smooth = `
    uniform float fresnelPower;
    varying vec3 vNormal;
    varying vec3 vViewPosition;
    varying vec3 vColor;
    void main() {
      vec3 viewDir = normalize(vViewPosition);
      vec3 normal = normalize(vNormal);
      float fresnel = pow(1.0 - abs(dot(viewDir, normal)), fresnelPower);
      float baseOpacity = 0.01 + fresnel * 0.5;
      float whiteness = min(min(vColor.r, vColor.g), vColor.b);
      float saturation = max(max(vColor.r, vColor.g), vColor.b) - whiteness;
      float colorBoost = 1.0 + saturation * 3.0;
      float whiteReduce = 1.0 - whiteness * 0.7;
      float opacity = baseOpacity * colorBoost * whiteReduce;
      gl_FragColor = vec4(vColor, opacity);
    }
  `;

  const fragment_flat = `
    uniform float fresnelPower;
    varying vec3 vViewPosition;
    varying vec3 vWorldPosition;
    varying vec3 vColor;
    void main() {
      vec3 viewDir = normalize(vViewPosition);
      vec3 dx = dFdx(vWorldPosition);
      vec3 dy = dFdy(vWorldPosition);
      vec3 normal = normalize(cross(dy, dx));
      if (dot(normal, viewDir) < 0.0) normal = -normal;
      float fresnel = pow(1.0 - abs(dot(viewDir, normal)), fresnelPower);
      float baseOpacity = 0.01 + fresnel * 0.5;
      float whiteness = min(min(vColor.r, vColor.g), vColor.b);
      float saturation = max(max(vColor.r, vColor.g), vColor.b) - whiteness;
      float colorBoost = 1.0 + saturation * 3.0;
      float whiteReduce = 1.0 - whiteness * 0.7;
      float opacity = baseOpacity * colorBoost * whiteReduce;
      gl_FragColor = vec4(vColor, opacity);
    }
  `;

  const vertex_smooth = `
    attribute vec3 color;
    varying vec3 vNormal;
    varying vec3 vViewPosition;
    varying vec3 vColor;
    void main() {
      vNormal = normalize(normalMatrix * normal);
      vColor = color;
      vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
      vViewPosition = -mvPosition.xyz;
      gl_Position = projectionMatrix * mvPosition;
    }
  `;

  const vertex_flat = `
    attribute vec3 color;
    varying vec3 vViewPosition;
    varying vec3 vWorldPosition;
    varying vec3 vColor;
    void main() {
      vColor = color;
      vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
      vViewPosition = -mvPosition.xyz;
      vWorldPosition = mvPosition.xyz;
      gl_Position = projectionMatrix * mvPosition;
    }
  `;

  return new THREE.ShaderMaterial({
    uniforms: {
      fresnelPower: { value: 2.0 },
    },
    vertexShader: flat_shading ? vertex_flat : vertex_smooth,
    fragmentShader: flat_shading ? fragment_flat : fragment_smooth,
    transparent: true,
    side: THREE.DoubleSide,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
  });
}

// Subtle axes
const axes_group = new THREE.Group();
const axis_len = 50;
const axis_mat = new THREE.LineBasicMaterial({ color: 0x333333 });
[[1, 0, 0], [0, 1, 0], [0, 0, 1]].forEach(dir => {
  const geo = new THREE.BufferGeometry().setFromPoints([
    new THREE.Vector3(0, 0, 0),
    new THREE.Vector3(dir[0] * axis_len, dir[1] * axis_len, dir[2] * axis_len)
  ]);
  axes_group.add(new THREE.Line(geo, axis_mat));
});

// Z axis label (small italic scientific style)
const label_canvas = document.createElement('canvas');
label_canvas.width = 32;
label_canvas.height = 32;
const label_ctx = label_canvas.getContext('2d')!;
label_ctx.fillStyle = '#555555';
label_ctx.font = 'italic 24px "Times New Roman", serif';
label_ctx.textAlign = 'center';
label_ctx.textBaseline = 'middle';
label_ctx.fillText('z', 16, 16);

const label_texture = new THREE.CanvasTexture(label_canvas);
const label_material = new THREE.SpriteMaterial({ map: label_texture, transparent: true });
const z_label = new THREE.Sprite(label_material);
z_label.position.set(0, 0, axis_len + 3);
z_label.scale.set(4, 4, 1);
axes_group.add(z_label);

scene.add(axes_group);

// Color map function (jet - blue to cyan to green to yellow to red)
function value_to_color(t: number): THREE.Color {
  t = Math.max(0, Math.min(1, t));

  let r: number, g: number, b: number;

  if (t < 0.125) {
    r = 0;
    g = 0;
    b = 0.5 + t * 4;
  } else if (t < 0.375) {
    r = 0;
    g = (t - 0.125) * 4;
    b = 1;
  } else if (t < 0.625) {
    r = (t - 0.375) * 4;
    g = 1;
    b = 1 - (t - 0.375) * 4;
  } else if (t < 0.875) {
    r = 1;
    g = 1 - (t - 0.625) * 4;
    b = 0;
  } else {
    r = 1 - (t - 0.875) * 2;
    g = 0;
    b = 0;
  }

  return new THREE.Color(
    Math.max(0, Math.min(1, r)),
    Math.max(0, Math.min(1, g)),
    Math.max(0, Math.min(1, b))
  );
}

// Viridis colormap (perceptually uniform, colorblind-friendly)
// dark purple -> blue -> green -> yellow
function value_to_color_viridis(t: number): THREE.Color {
  t = Math.max(0, Math.min(1, t));

  let r: number, g: number, b: number;

  if (t < 0.25) {
    const s = t / 0.25;
    r = 0.267 + s * (0.282 - 0.267);
    g = 0.004 + s * (0.140 - 0.004);
    b = 0.329 + s * (0.458 - 0.329);
  } else if (t < 0.5) {
    const s = (t - 0.25) / 0.25;
    r = 0.282 + s * (0.127 - 0.282);
    g = 0.140 + s * (0.566 - 0.140);
    b = 0.458 + s * (0.551 - 0.458);
  } else if (t < 0.75) {
    const s = (t - 0.5) / 0.25;
    r = 0.127 + s * (0.741 - 0.127);
    g = 0.566 + s * (0.873 - 0.566);
    b = 0.551 + s * (0.150 - 0.551);
  } else {
    const s = (t - 0.75) / 0.25;
    r = 0.741 + s * (0.993 - 0.741);
    g = 0.873 + s * (0.906 - 0.873);
    b = 0.150 + s * (0.144 - 0.150);
  }

  return new THREE.Color(r, g, b);
}

// Plasma colormap (perceptually uniform)
// dark purple -> pink -> orange -> yellow
function value_to_color_plasma(t: number): THREE.Color {
  t = Math.max(0, Math.min(1, t));

  let r: number, g: number, b: number;

  if (t < 0.25) {
    const s = t / 0.25;
    r = 0.050 + s * (0.417 - 0.050);
    g = 0.030 + s * (0.001 - 0.030);
    b = 0.528 + s * (0.659 - 0.528);
  } else if (t < 0.5) {
    const s = (t - 0.25) / 0.25;
    r = 0.417 + s * (0.798 - 0.417);
    g = 0.001 + s * (0.279 - 0.001);
    b = 0.659 + s * (0.470 - 0.659);
  } else if (t < 0.75) {
    const s = (t - 0.5) / 0.25;
    r = 0.798 + s * (0.973 - 0.798);
    g = 0.279 + s * (0.580 - 0.279);
    b = 0.470 + s * (0.254 - 0.470);
  } else {
    const s = (t - 0.75) / 0.25;
    r = 0.973 + s * (0.940 - 0.973);
    g = 0.580 + s * (0.975 - 0.580);
    b = 0.254 + s * (0.131 - 0.254);
  }

  return new THREE.Color(r, g, b);
}

type ColormapFn = (t: number) => THREE.Color;

// Parse binary mesh
function parse_binary_mesh(buffer: ArrayBuffer): { geometry: THREE.BufferGeometry, edges: number } | null {
  const view = new DataView(buffer);

  if (buffer.byteLength < 8) return null;

  const numVertices = view.getUint32(0, true);
  const numIndices = view.getUint32(4, true);

  if (numVertices === 0 || numIndices === 0) return null;

  const positionsOffset = 8;
  const normalsOffset = positionsOffset + numVertices * 3 * 4;
  const colorsOffset = normalsOffset + numVertices * 3 * 4;
  const indicesOffset = colorsOffset + numVertices * 3 * 4;

  const expectedSize = indicesOffset + numIndices * 4;
  if (buffer.byteLength < expectedSize) {
    console.error(`Buffer too small: ${buffer.byteLength} < ${expectedSize}`);
    return null;
  }

  const positions = new Float32Array(buffer, positionsOffset, numVertices * 3);
  const normals = new Float32Array(buffer, normalsOffset, numVertices * 3);
  const colors = new Float32Array(buffer, colorsOffset, numVertices * 3);
  const indices = new Uint32Array(buffer, indicesOffset, numIndices);

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.BufferAttribute(positions.slice(), 3));
  geometry.setAttribute('normal', new THREE.BufferAttribute(normals.slice(), 3));
  geometry.setAttribute('color', new THREE.BufferAttribute(colors.slice(), 3));
  geometry.setIndex(new THREE.BufferAttribute(indices.slice(), 1));

  // Triangles have 3 edges each (though shared edges counted once would be ~1.5x triangles)
  const numTriangles = numIndices / 3;
  const numEdges = numTriangles * 3; // Total edge count (including shared)

  console.log(`Mesh: ${numVertices} vertices, ${numTriangles} triangles, ${numEdges} edges`);

  return { geometry, edges: numEdges };
}

function create_edge_overlay(geometry: THREE.BufferGeometry): THREE.LineSegments {
  const wire = new THREE.WireframeGeometry(geometry);
  const mat = new THREE.LineBasicMaterial({
    color: 0x80f5ff,
    transparent: true,
    opacity: 0.2,
    depthWrite: false,
  });
  const lines = new THREE.LineSegments(wire, mat);
  lines.renderOrder = 10;
  return lines;
}

function set_download_button_enabled(enabled: boolean) {
  if (!download_stl_btn) return;
  download_stl_btn.disabled = !enabled;
}

function refresh_download_button_state() {
  set_download_button_enabled(Boolean(current_mesh) || current_export_files.length > 0);
}

function build_binary_stl(mesh: THREE.Mesh): Blob {
  const geometry = mesh.geometry as THREE.BufferGeometry;
  const position = geometry.getAttribute('position') as THREE.BufferAttribute;
  const index = geometry.getIndex();
  const triangle_count = index ? index.count / 3 : position.count / 3;

  const buffer = new ArrayBuffer(84 + triangle_count * 50);
  const view = new DataView(buffer);
  view.setUint32(80, triangle_count, true);

  const a = new THREE.Vector3();
  const b = new THREE.Vector3();
  const c = new THREE.Vector3();
  const ab = new THREE.Vector3();
  const ac = new THREE.Vector3();
  const normal = new THREE.Vector3();
  const mesh_world = mesh.matrixWorld;

  let offset = 84;
  for (let t = 0; t < triangle_count; t++) {
    const i0 = index ? index.getX(t * 3) : t * 3;
    const i1 = index ? index.getX(t * 3 + 1) : t * 3 + 1;
    const i2 = index ? index.getX(t * 3 + 2) : t * 3 + 2;

    a.fromBufferAttribute(position, i0).applyMatrix4(mesh_world);
    b.fromBufferAttribute(position, i1).applyMatrix4(mesh_world);
    c.fromBufferAttribute(position, i2).applyMatrix4(mesh_world);

    ab.subVectors(b, a);
    ac.subVectors(c, a);
    normal.crossVectors(ab, ac).normalize();

    view.setFloat32(offset, normal.x, true); offset += 4;
    view.setFloat32(offset, normal.y, true); offset += 4;
    view.setFloat32(offset, normal.z, true); offset += 4;

    view.setFloat32(offset, a.x, true); offset += 4;
    view.setFloat32(offset, a.y, true); offset += 4;
    view.setFloat32(offset, a.z, true); offset += 4;

    view.setFloat32(offset, b.x, true); offset += 4;
    view.setFloat32(offset, b.y, true); offset += 4;
    view.setFloat32(offset, b.z, true); offset += 4;

    view.setFloat32(offset, c.x, true); offset += 4;
    view.setFloat32(offset, c.y, true); offset += 4;
    view.setFloat32(offset, c.z, true); offset += 4;

    view.setUint16(offset, 0, true); offset += 2;
  }

  return new Blob([buffer], { type: 'model/stl' });
}

function download_current_mesh_stl() {
  if (!current_mesh) return;

  const base = server_info?.file ? (server_info.file.split('/').pop() || 'mesh') : 'mesh';
  const filename = base.replace(/\.lua$/i, '') + '.stl';
  const blob = build_binary_stl(current_mesh);
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

function try_handle_export_packet(buffer: ArrayBuffer): boolean {
  if (buffer.byteLength < 17) return false;
  const magic = String.fromCharCode(...new Uint8Array(buffer, 0, 8));
  if (!magic.startsWith('EXPORT')) return false;

  const view = new DataView(buffer);
  let offset = 8;
  const _formatId = view.getUint8(offset); offset += 1;
  const nameLen = view.getUint32(offset, true); offset += 4;
  if (buffer.byteLength < offset + nameLen + 4) return true;

  const nameBytes = new Uint8Array(buffer, offset, nameLen);
  const filename = new TextDecoder().decode(nameBytes);
  offset += nameLen;

  const payloadLen = view.getUint32(offset, true); offset += 4;
  if (buffer.byteLength < offset + payloadLen) return true;
  const payload = buffer.slice(offset, offset + payloadLen);

  const blob = new Blob([payload], {
    type: filename.toLowerCase().endsWith('.zip') ? 'application/zip' : 'model/stl'
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename || 'exports.zip';
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
  return true;
}

function request_export_download() {
  if (!current_ws || current_ws.readyState !== WebSocket.OPEN) return;
  current_ws.send(JSON.stringify({ type: 'download_exports' }));
}

// Plane type constants matching server
const PLANE_XZ = 0;
const PLANE_XY = 1;
const PLANE_YZ = 2;

// Colormap constants matching server
const COLORMAP_JET = 0;
const COLORMAP_VIRIDIS = 1;
const COLORMAP_PLASMA = 2;

function get_colormap_fn(colormap_id: number): ColormapFn {
  switch (colormap_id) {
    case COLORMAP_VIRIDIS:
      return value_to_color_viridis;
    case COLORMAP_PLASMA:
      return value_to_color_plasma;
    default:
      return value_to_color;
  }
}

// Parse binary field data
function parse_field_data(buffer: ArrayBuffer) {
  const view = new DataView(buffer);
  let offset = 8; // Skip "FIELD\0\0\0" header

  // 2D slice dimensions
  const sliceWidth = view.getUint32(offset, true); offset += 4;
  const sliceHeight = view.getUint32(offset, true); offset += 4;

  // 2D slice bounds [axis1_min, axis1_max, axis2_min, axis2_max]
  const axis1Min = view.getFloat32(offset, true); offset += 4;
  const axis1Max = view.getFloat32(offset, true); offset += 4;
  const axis2Min = view.getFloat32(offset, true); offset += 4;
  const axis2Max = view.getFloat32(offset, true); offset += 4;

  // Plane type (u8), offset (f32), colormap (u8)
  const planeType = view.getUint8(offset); offset += 1;
  const planeOffset = view.getFloat32(offset, true); offset += 4;
  const colormapId = view.getUint8(offset); offset += 1;

  const sliceSize = sliceWidth * sliceHeight;

  // 2D slice data
  const sliceBx = new Float32Array(buffer, offset, sliceSize);
  offset += sliceSize * 4;
  const sliceBz = new Float32Array(buffer, offset, sliceSize);
  offset += sliceSize * 4;
  const sliceMagnitude = new Float32Array(buffer, offset, sliceSize);
  offset += sliceSize * 4;

  // 3D arrows
  const numArrows = view.getUint32(offset, true); offset += 4;

  const arrowPositions = new Float32Array(buffer, offset, numArrows * 3);
  offset += numArrows * 3 * 4;
  const arrowVectors = new Float32Array(buffer, offset, numArrows * 3);
  offset += numArrows * 3 * 4;
  const arrowMagnitudes = new Float32Array(buffer, offset, numArrows);
  offset += numArrows * 4;

  // 1D line
  const linePoints = view.getUint32(offset, true); offset += 4;

  const lineZ = new Float32Array(buffer, offset, linePoints);
  offset += linePoints * 4;
  const lineBz = new Float32Array(buffer, offset, linePoints);

  return {
    slice: {
      width: sliceWidth,
      height: sliceHeight,
      bounds: [axis1Min, axis1Max, axis2Min, axis2Max],
      plane_type: planeType,
      plane_offset: planeOffset,
      colormap: get_colormap_fn(colormapId),
      bx: sliceBx,
      bz: sliceBz,
      magnitude: sliceMagnitude,
    },
    arrows: {
      positions: arrowPositions,
      vectors: arrowVectors,
      magnitudes: arrowMagnitudes,
    },
    line: {
      z: lineZ,
      bz: lineBz,
    },
  };
}

// Create 3D arrow field visualization
function create_arrow_field(arrows: { positions: Float32Array, vectors: Float32Array, magnitudes: Float32Array }) {
  const group = new THREE.Group();

  const numArrows = arrows.positions.length / 3;
  const maxMag = Math.max(...Array.from(arrows.magnitudes));

  // Create instanced mesh for efficiency
  const arrowLength = 8;
  const arrowGeo = new THREE.ConeGeometry(1.5, arrowLength, 8);
  arrowGeo.translate(0, arrowLength / 2, 0);
  arrowGeo.rotateX(Math.PI / 2); // Point along +Z by default

  for (let i = 0; i < numArrows; i++) {
    const px = arrows.positions[i * 3];
    const py = arrows.positions[i * 3 + 1];
    const pz = arrows.positions[i * 3 + 2];

    const vx = arrows.vectors[i * 3];
    const vy = arrows.vectors[i * 3 + 1];
    const vz = arrows.vectors[i * 3 + 2];

    const mag = arrows.magnitudes[i];
    const t = mag / maxMag;

    const color = value_to_color(t);
    const material = new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.8 });

    const arrow = new THREE.Mesh(arrowGeo, material);
    arrow.position.set(px, py, pz);

    // Orient arrow along field direction
    const dir = new THREE.Vector3(vx, vy, vz).normalize();
    const quaternion = new THREE.Quaternion();
    quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), dir);
    arrow.quaternion.copy(quaternion);

    // Scale by magnitude
    const scale = 0.3 + 0.7 * t;
    arrow.scale.setScalar(scale);

    group.add(arrow);
  }

  return group;
}

// Create 2D field plane visualization
function create_field_plane(slice: { width: number, height: number, bounds: number[], plane_type: number, plane_offset: number, colormap: ColormapFn, magnitude: Float32Array }): THREE.Mesh {
  const { width, height, bounds, plane_type, plane_offset, colormap, magnitude } = slice;
  const [axis1Min, axis1Max, axis2Min, axis2Max] = bounds;

  // Create texture from magnitude data
  const textureData = new Uint8Array(width * height * 4);
  const maxMag = Math.max(...Array.from(magnitude));

  for (let i = 0; i < width * height; i++) {
    const mag = magnitude[i];

    // Flip Y coordinate for texture
    const row = Math.floor(i / width);
    const col = i % width;
    const flippedIdx = (height - 1 - row) * width + col;

    // Make zero-magnitude pixels transparent (outside medium)
    if (mag < 1e-6 || maxMag < 1e-6) {
      textureData[flippedIdx * 4] = 0;
      textureData[flippedIdx * 4 + 1] = 0;
      textureData[flippedIdx * 4 + 2] = 0;
      textureData[flippedIdx * 4 + 3] = 0;
    } else {
      const t = mag / maxMag;
      const color = colormap(t);
      textureData[flippedIdx * 4] = Math.floor(color.r * 255);
      textureData[flippedIdx * 4 + 1] = Math.floor(color.g * 255);
      textureData[flippedIdx * 4 + 2] = Math.floor(color.b * 255);
      textureData[flippedIdx * 4 + 3] = 220;
    }
  }

  const texture = new THREE.DataTexture(textureData, width, height, THREE.RGBAFormat);
  texture.needsUpdate = true;

  const plane_width = axis1Max - axis1Min;
  const plane_height = axis2Max - axis2Min;

  const geometry = new THREE.PlaneGeometry(plane_width, plane_height);
  const material = new THREE.MeshBasicMaterial({
    map: texture,
    transparent: true,
    side: THREE.DoubleSide,
    depthWrite: false,
  });

  const plane = new THREE.Mesh(geometry, material);

  // Position and orient plane based on plane type
  // PlaneGeometry creates a plane in XY by default (facing +Z)
  const axis1_center = (axis1Min + axis1Max) / 2;
  const axis2_center = (axis2Min + axis2Max) / 2;

  if (plane_type === PLANE_XZ) {
    // XZ plane at Y=offset: rotate -90 around X
    plane.position.set(axis1_center, plane_offset, axis2_center);
    plane.rotation.x = -Math.PI / 2;
  } else if (plane_type === PLANE_XY) {
    // XY plane at Z=offset: no rotation needed
    plane.position.set(axis1_center, axis2_center, plane_offset);
  } else if (plane_type === PLANE_YZ) {
    // YZ plane at X=offset: rotate 90 around Y
    plane.position.set(plane_offset, axis1_center, axis2_center);
    plane.rotation.y = Math.PI / 2;
  }

  return plane;
}

// Draw 1D graph
function draw_graph(line: { z: Float32Array, bz: Float32Array }) {
  const ctx = graph_ctx;
  const width = graph_canvas.width;
  const height = graph_canvas.height;

  // Show the graph window
  openWindow('graph-window');

  // Clear
  ctx.fillStyle = 'rgba(0, 0, 0, 0.9)';
  ctx.fillRect(0, 0, width, height);

  // Find max |Bz|
  let max_bz = 0;
  let min_bz = 0;
  for (let i = 0; i < line.bz.length; i++) {
    max_bz = Math.max(max_bz, line.bz[i]);
    min_bz = Math.min(min_bz, line.bz[i]);
  }

  const z_min = line.z[0];
  const z_max = line.z[line.z.length - 1];

  // Draw axes
  ctx.strokeStyle = '#666';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(40, 10);
  ctx.lineTo(40, height - 30);
  ctx.lineTo(width - 10, height - 30);
  ctx.stroke();

  // Draw zero line
  const zero_y = height - 30 - ((0 - min_bz) / (max_bz - min_bz)) * (height - 50);
  ctx.strokeStyle = '#444';
  ctx.setLineDash([3, 3]);
  ctx.beginPath();
  ctx.moveTo(40, zero_y);
  ctx.lineTo(width - 10, zero_y);
  ctx.stroke();
  ctx.setLineDash([]);

  // Draw data (white line for 80s TUI look)
  ctx.strokeStyle = '#ccc';
  ctx.lineWidth = 1;
  ctx.beginPath();

  for (let i = 0; i < line.z.length; i++) {
    const x = 40 + ((line.z[i] - z_min) / (z_max - z_min)) * (width - 60);
    const y = height - 30 - ((line.bz[i] - min_bz) / (max_bz - min_bz)) * (height - 50);

    if (i === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  }
  ctx.stroke();

  // Labels
  ctx.fillStyle = '#888';
  ctx.font = '10px "Courier New", monospace';
  ctx.fillText('Bz (mT)', 5, 15);
  ctx.fillText(`${(max_bz * 1000).toFixed(2)}`, 5, 28);
  ctx.fillText(`${(min_bz * 1000).toFixed(2)}`, 5, height - 35);
  ctx.fillText('Z (mm)', width - 50, height - 5);
  ctx.fillText(`${z_min.toFixed(0)}`, 35, height - 15);
  ctx.fillText(`${z_max.toFixed(0)}`, width - 30, height - 15);

  // Max field indicator
  ctx.fillStyle = '#ccc';
  ctx.font = '10px "Courier New", monospace';
  ctx.fillText(`MAX: ${(max_bz * 1000).toFixed(3)} mT`, width - 130, 15);

  // Center field
  const center_idx = Math.floor(line.z.length / 2);
  ctx.fillText(`CTR: ${(line.bz[center_idx] * 1000).toFixed(3)} mT`, width - 130, 28);
}

// Parse NanoVNA data
function parse_nanovna_data(buffer: ArrayBuffer) {
  const view = new DataView(buffer);
  let offset = 8; // Skip "NANOVNA\0" header

  const num_points = view.getUint32(offset, true); offset += 4;
  const min_s11_db = view.getFloat32(offset, true); offset += 4;
  const min_s11_freq = view.getFloat32(offset, true); offset += 4;

  const frequencies = new Float32Array(num_points);
  const magnitudes = new Float32Array(num_points);
  const phases = new Float32Array(num_points);

  for (let i = 0; i < num_points; i++) {
    frequencies[i] = view.getFloat32(offset, true);
    offset += 4;
  }

  for (let i = 0; i < num_points; i++) {
    magnitudes[i] = view.getFloat32(offset, true);
    offset += 4;
  }

  for (let i = 0; i < num_points; i++) {
    phases[i] = view.getFloat32(offset, true);
    offset += 4;
  }

  return {
    num_points,
    min_s11_db,
    min_s11_freq,
    frequencies,
    magnitudes,
    phases,
  };
}

// Parse point measurement data (GaussMeter/Hydrophone)
function parse_measure_data(buffer: ArrayBuffer) {
  const view = new DataView(buffer);
  let offset = 8; // Skip "MEASURE\0" header

  const position: [number, number, number] = [
    view.getFloat32(offset, true),
    view.getFloat32(offset + 4, true),
    view.getFloat32(offset + 8, true),
  ];
  offset += 12;

  const value: [number, number, number] = [
    view.getFloat32(offset, true),
    view.getFloat32(offset + 4, true),
    view.getFloat32(offset + 8, true),
  ];
  offset += 12;

  const magnitude = view.getFloat32(offset, true);
  offset += 4;

  const label_len = view.getUint32(offset, true);
  offset += 4;

  const label_bytes = new Uint8Array(buffer, offset, label_len);
  const label = new TextDecoder().decode(label_bytes);

  return { position, value, magnitude, label };
}

// Parse line probe data
function parse_lnprobe_data(buffer: ArrayBuffer) {
  const view = new DataView(buffer);
  let offset = 8; // Skip "LNPROBE\0" header

  const num_points = view.getUint32(offset, true);
  offset += 4;

  const start: [number, number, number] = [
    view.getFloat32(offset, true),
    view.getFloat32(offset + 4, true),
    view.getFloat32(offset + 8, true),
  ];
  offset += 12;

  const stop: [number, number, number] = [
    view.getFloat32(offset, true),
    view.getFloat32(offset + 4, true),
    view.getFloat32(offset + 8, true),
  ];
  offset += 12;

  const positions = new Float32Array(num_points * 3);
  for (let i = 0; i < num_points * 3; i++) {
    positions[i] = view.getFloat32(offset, true);
    offset += 4;
  }

  const values = new Float32Array(num_points * 3);
  for (let i = 0; i < num_points * 3; i++) {
    values[i] = view.getFloat32(offset, true);
    offset += 4;
  }

  const magnitudes = new Float32Array(num_points);
  for (let i = 0; i < num_points; i++) {
    magnitudes[i] = view.getFloat32(offset, true);
    offset += 4;
  }

  const name_len = view.getUint32(offset, true);
  offset += 4;

  const name_bytes = new Uint8Array(buffer, offset, name_len);
  const name = new TextDecoder().decode(name_bytes);

  return { num_points, start, stop, positions, values, magnitudes, name };
}

// Display point measurement as 3D marker
function display_measurement(data: { position: [number, number, number], value: [number, number, number], magnitude: number, label: string }) {
  if (!measurement_markers) {
    measurement_markers = new THREE.Group();
    scene.add(measurement_markers);
  }

  const [x, y, z] = data.position;

  // Create sphere marker at measurement position
  const marker_geo = new THREE.SphereGeometry(2, 16, 16);
  const marker_mat = new THREE.MeshBasicMaterial({ color: 0x00ff00, transparent: true, opacity: 0.8 });
  const marker = new THREE.Mesh(marker_geo, marker_mat);
  marker.position.set(x, y, z);
  measurement_markers.add(marker);

  // Create text sprite for label
  const sprite_canvas = document.createElement('canvas');
  sprite_canvas.width = 256;
  sprite_canvas.height = 64;
  const ctx = sprite_canvas.getContext('2d')!;
  ctx.fillStyle = 'rgba(0, 0, 0, 0.7)';
  ctx.fillRect(0, 0, 256, 64);
  ctx.fillStyle = '#0f0';
  ctx.font = '14px Courier New';
  ctx.textAlign = 'center';
  ctx.fillText(data.label, 128, 24);
  ctx.fillText(`${(data.magnitude * 1000).toFixed(3)} mT`, 128, 44);

  const sprite_texture = new THREE.CanvasTexture(sprite_canvas);
  const sprite_mat = new THREE.SpriteMaterial({ map: sprite_texture, transparent: true });
  const sprite = new THREE.Sprite(sprite_mat);
  sprite.position.set(x, y, z + 8);
  sprite.scale.set(25, 6, 1);
  measurement_markers.add(sprite);

  console.log(`Measurement '${data.label}': pos=(${x.toFixed(1)}, ${y.toFixed(1)}, ${z.toFixed(1)}), |B|=${(data.magnitude * 1000).toFixed(3)} mT`);
}

// Display line probe as 3D line
function display_probe_line(data: { num_points: number, start: [number, number, number], stop: [number, number, number], positions: Float32Array, magnitudes: Float32Array, name: string }) {
  if (!probe_lines) {
    probe_lines = new THREE.Group();
    scene.add(probe_lines);
  }

  // Create line geometry
  const line_points: THREE.Vector3[] = [];
  for (let i = 0; i < data.num_points; i++) {
    line_points.push(new THREE.Vector3(
      data.positions[i * 3],
      data.positions[i * 3 + 1],
      data.positions[i * 3 + 2]
    ));
  }

  const line_geo = new THREE.BufferGeometry().setFromPoints(line_points);
  const line_mat = new THREE.LineBasicMaterial({ color: 0x00ffff, linewidth: 2 });
  const line = new THREE.Line(line_geo, line_mat);
  probe_lines.add(line);

  // Add endpoint markers
  const start_marker = new THREE.Mesh(
    new THREE.SphereGeometry(1.5, 12, 12),
    new THREE.MeshBasicMaterial({ color: 0x00ff00 })
  );
  start_marker.position.set(data.start[0], data.start[1], data.start[2]);
  probe_lines.add(start_marker);

  const end_marker = new THREE.Mesh(
    new THREE.SphereGeometry(1.5, 12, 12),
    new THREE.MeshBasicMaterial({ color: 0xff0000 })
  );
  end_marker.position.set(data.stop[0], data.stop[1], data.stop[2]);
  probe_lines.add(end_marker);

  console.log(`Probe '${data.name}': ${data.num_points} points from (${data.start[0].toFixed(1)}, ${data.start[1].toFixed(1)}, ${data.start[2].toFixed(1)}) to (${data.stop[0].toFixed(1)}, ${data.stop[1].toFixed(1)}, ${data.stop[2].toFixed(1)})`);
}

// Draw NanoVNA S11 graph
function draw_nanovna(data: { frequencies: Float32Array, magnitudes: Float32Array, min_s11_db: number, min_s11_freq: number }) {
  const ctx = nanovna_ctx;
  const width = nanovna_canvas.width;
  const height = nanovna_canvas.height;

  openWindow('nanovna-window');

  // Clear
  ctx.fillStyle = 'rgba(0, 0, 0, 0.9)';
  ctx.fillRect(0, 0, width, height);

  const f_min = data.frequencies[0];
  const f_max = data.frequencies[data.frequencies.length - 1];

  // Find magnitude range
  let mag_min = 0;
  let mag_max = -60;
  for (let i = 0; i < data.magnitudes.length; i++) {
    mag_max = Math.max(mag_max, data.magnitudes[i]);
    mag_min = Math.min(mag_min, data.magnitudes[i]);
  }
  mag_min = Math.floor(mag_min / 10) * 10;
  mag_max = 0;

  // Draw axes
  ctx.strokeStyle = '#666';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(40, 10);
  ctx.lineTo(40, height - 30);
  ctx.lineTo(width - 10, height - 30);
  ctx.stroke();

  // Draw -3dB reference line
  const ref_y = height - 30 - ((-3 - mag_min) / (mag_max - mag_min)) * (height - 50);
  ctx.strokeStyle = '#444';
  ctx.setLineDash([3, 3]);
  ctx.beginPath();
  ctx.moveTo(40, ref_y);
  ctx.lineTo(width - 10, ref_y);
  ctx.stroke();
  ctx.setLineDash([]);

  // Draw S11 magnitude
  ctx.strokeStyle = '#0f0';
  ctx.lineWidth = 1;
  ctx.beginPath();

  for (let i = 0; i < data.frequencies.length; i++) {
    const x = 40 + ((data.frequencies[i] - f_min) / (f_max - f_min)) * (width - 60);
    const y = height - 30 - ((data.magnitudes[i] - mag_min) / (mag_max - mag_min)) * (height - 50);

    if (i === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  }
  ctx.stroke();

  // Mark minimum point
  const min_x = 40 + ((data.min_s11_freq - f_min) / (f_max - f_min)) * (width - 60);
  const min_y = height - 30 - ((data.min_s11_db - mag_min) / (mag_max - mag_min)) * (height - 50);
  ctx.fillStyle = '#f00';
  ctx.beginPath();
  ctx.arc(min_x, min_y, 4, 0, Math.PI * 2);
  ctx.fill();

  // Labels
  ctx.fillStyle = '#888';
  ctx.font = '10px "Courier New", monospace';
  ctx.fillText('S11 (dB)', 5, 15);
  ctx.fillText(`${mag_max}`, 5, 28);
  ctx.fillText(`${mag_min}`, 5, height - 35);
  ctx.fillText('Freq (MHz)', width - 70, height - 5);
  ctx.fillText(`${(f_min / 1e6).toFixed(1)}`, 35, height - 15);
  ctx.fillText(`${(f_max / 1e6).toFixed(1)}`, width - 40, height - 15);

  // Min S11 indicator
  ctx.fillStyle = '#0f0';
  ctx.font = '10px "Courier New", monospace';
  ctx.fillText(`MIN: ${data.min_s11_db.toFixed(1)} dB @ ${(data.min_s11_freq / 1e6).toFixed(2)} MHz`, width - 180, 15);
}

// Parse circuit data
function parse_circuit_data(buffer: ArrayBuffer) {
  const view = new DataView(buffer);
  let offset = 8; // Skip "CIRCUIT\0" header

  // Size (width, height)
  const width = view.getFloat32(offset, true); offset += 4;
  const height = view.getFloat32(offset, true); offset += 4;

  // SVG length and data
  const svg_len = view.getUint32(offset, true); offset += 4;
  const svg_bytes = new Uint8Array(buffer, offset, svg_len);
  const svg_string = new TextDecoder().decode(svg_bytes);

  return {
    size: { width, height },
    svg: svg_string,
  };
}

// Display circuit as 2D overlay
function display_circuit_overlay(circuit: { size: { width: number, height: number }, svg: string }) {
  circuit_overlay.innerHTML = circuit.svg;
  circuit_overlay.classList.add('visible');
  circuit_visible = true;
  circuit_width = circuit.size.width;
}

function update_mesh(buffer: ArrayBuffer) {
  // Check header to determine data type
  const header = new Uint8Array(buffer, 0, 8);
  const header_5 = String.fromCharCode(...header.slice(0, 5));
  const header_8 = String.fromCharCode(...header);

  // Handle view config
  if (header_8.startsWith('VIEW')) {
    const view = new DataView(buffer);
    let offset = 8;
    flat_shading = view.getUint8(offset) === 1;
    offset += 1;

    // VIEW packet format: [flat_shading][show_edges][has_camera]
    let has_camera = false;
    show_edges = view.getUint8(offset) === 1;
    offset += 1;
    has_camera = view.getUint8(offset) === 1;
    offset += 1;

    if (has_camera && buffer.byteLength >= offset + 36) {
      const cam_x = view.getFloat32(offset, true); offset += 4;
      const cam_y = view.getFloat32(offset, true); offset += 4;
      const cam_z = view.getFloat32(offset, true); offset += 4;
      const tgt_x = view.getFloat32(offset, true); offset += 4;
      const tgt_y = view.getFloat32(offset, true); offset += 4;
      const tgt_z = view.getFloat32(offset, true); offset += 4;
      const fov = view.getFloat32(offset, true); offset += 4;
      const near = view.getFloat32(offset, true); offset += 4;
      const far = view.getFloat32(offset, true);

      console.log(`[VIEW] Received: near=${near}, far=${far}, fov=${fov}`);

      // Store as home camera position (including near/far for clipping)
      home_camera = { pos: [cam_x, cam_y, cam_z], target: [tgt_x, tgt_y, tgt_z], fov, near, far };

      // Only apply Lua camera if it changed (preserves user manipulation)
      const lua_key = `${cam_x.toFixed(2)},${cam_y.toFixed(2)},${cam_z.toFixed(2)},${tgt_x.toFixed(2)},${tgt_y.toFixed(2)},${tgt_z.toFixed(2)},${fov.toFixed(1)},${near.toFixed(2)},${far.toFixed(0)}`;
      if (lua_key !== last_lua_camera_key) {
        camera.position.set(cam_x, cam_y, cam_z);
        controls.target.set(tgt_x, tgt_y, tgt_z);
        camera.fov = fov;
        camera.near = near;
        camera.far = far;
        camera.updateProjectionMatrix();
        controls.update();
        last_lua_camera_key = lua_key;
        // Clear localStorage when Lua explicitly sets camera
        localStorage.removeItem('mittens_camera');
        console.log(`View config: flat_shading=${flat_shading}, show_edges=${show_edges}, camera=(${cam_x}, ${cam_y}, ${cam_z}), target=(${tgt_x}, ${tgt_y}, ${tgt_z}), fov=${fov}, near=${near}, far=${far}`);
      } else {
        console.log(`View config: flat_shading=${flat_shading}, show_edges=${show_edges}, camera unchanged (keeping user position)`);
      }
    } else {
      console.log(`View config: flat_shading=${flat_shading}, show_edges=${show_edges}, no camera specified`);
    }
    return;
  }

  // Handle circuit data
  if (header_8 === 'CIRCUIT\0') {
    const circuit_data = parse_circuit_data(buffer);
    console.log(`Circuit data: ${circuit_data.size.width}x${circuit_data.size.height}`);
    display_circuit_overlay(circuit_data);
    return;
  }

  // Handle NanoVNA data
  if (header_8 === 'NANOVNA\0') {
    const nanovna_data = parse_nanovna_data(buffer);
    console.log(`NanoVNA data: ${nanovna_data.num_points} points, min S11: ${nanovna_data.min_s11_db.toFixed(1)} dB @ ${(nanovna_data.min_s11_freq / 1e6).toFixed(2)} MHz`);
    draw_nanovna(nanovna_data);
    return;
  }

  // Handle point measurement data (GaussMeter)
  if (header_8 === 'MEASURE\0') {
    const measure_data = parse_measure_data(buffer);
    display_measurement(measure_data);
    return;
  }

  // Handle line probe data
  if (header_8 === 'LNPROBE\0') {
    const probe_data = parse_lnprobe_data(buffer);
    display_probe_line(probe_data);
    return;
  }

  if (header_5 === 'FIELD') {
    // Parse and display field data
    const fieldData = parse_field_data(buffer);
    console.log(`Field data: ${fieldData.slice.width}x${fieldData.slice.height} slice, ${fieldData.arrows.positions.length / 3} arrows`);

    // Track field grid "edges" (grid cells * 4 edges each)
    const gridCells = (fieldData.slice.width - 1) * (fieldData.slice.height - 1);
    last_edge_count = gridCells * 4 + (fieldData.arrows.positions.length / 3) * 8; // arrows have ~8 edges each

    // Remove old visualizations
    if (arrows_group) {
      scene.remove(arrows_group);
      arrows_group = null;
    }
    if (field_plane) {
      scene.remove(field_plane);
      field_plane = null;
    }

    // Create arrow field only if there are arrows
    if (fieldData.arrows.positions.length > 0) {
      arrows_group = create_arrow_field(fieldData.arrows);
      scene.add(arrows_group);
    }

    field_plane = create_field_plane(fieldData.slice);
    scene.add(field_plane);

    // Draw 1D graph only if there's line data
    if (fieldData.line.z.length > 0) {
      draw_graph(fieldData.line);
    }

    return;
  }

  // Regular mesh data - clear old mesh AND any field visualizations
  if (current_mesh) {
    current_mesh.geometry.dispose();
    if (current_mesh.material instanceof THREE.Material) {
      current_mesh.material.dispose();
    }
    scene.remove(current_mesh);
    current_mesh = null;
    refresh_download_button_state();
  }
  if (current_edges) {
    current_edges.geometry.dispose();
    if (current_edges.material instanceof THREE.Material) {
      current_edges.material.dispose();
    }
    scene.remove(current_edges);
    current_edges = null;
  }

  // Clear field visualizations from previous file
  if (arrows_group) {
    scene.remove(arrows_group);
    arrows_group = null;
  }
  if (field_plane) {
    scene.remove(field_plane);
    field_plane = null;
  }
  // Clear measurement markers
  if (measurement_markers) {
    scene.remove(measurement_markers);
    measurement_markers = null;
  }
  // Clear probe lines
  if (probe_lines) {
    scene.remove(probe_lines);
    probe_lines = null;
  }
  // Hide circuit overlay
  circuit_overlay.classList.remove('visible');
  circuit_overlay.innerHTML = '';
  circuit_visible = false;
  // Hide graph window
  graph_window.classList.remove('visible');

  const result = parse_binary_mesh(buffer);
  if (!result) return;

  last_edge_count = result.edges;

  const material = create_xray_material(flat_shading);
  current_mesh = new THREE.Mesh(result.geometry, material);
  scene.add(current_mesh);
  refresh_download_button_state();
  if (show_edges) {
    current_edges = create_edge_overlay(result.geometry);
    scene.add(current_edges);
  }
}

// Update 3D text labels
interface Label {
  text: string;
  x: number;
  y: number;
  z: number;
  size?: number;
  color?: string;
  ops?: LabelOp[];
}

interface LabelOp {
  op: string;
  x?: number;
  y?: number;
  z?: number;
}

function update_labels(labels: Label[]) {
  // Clear existing labels
  while (labels_group.children.length > 0) {
    const child = labels_group.children[0];
    if ((child as any).dispose) (child as any).dispose();
    labels_group.remove(child);
  }

  // Create new labels
  for (const label of labels) {
    const text = new Text();
    text.text = label.text;
    text.fontSize = label.size || 5;
    text.color = label.color || 0xffffff;
    text.anchorX = 'center';
    text.anchorY = 'middle';

    if (label.ops && label.ops.length > 0) {
      // Apply the same op sequence as geometry, with default text-facing orientation.
      const transform = new THREE.Matrix4().makeRotationX(Math.PI / 2);

      for (const op of label.ops) {
        if (op.op === 'translate') {
          transform.multiply(new THREE.Matrix4().makeTranslation(
            op.x || 0,
            op.y || 0,
            op.z || 0,
          ));
        } else if (op.op === 'rotate') {
          const rotation = new THREE.Euler(
            THREE.MathUtils.degToRad(op.x || 0),
            THREE.MathUtils.degToRad(op.y || 0),
            THREE.MathUtils.degToRad(op.z || 0),
            'XYZ'
          );
          transform.multiply(new THREE.Matrix4().makeRotationFromEuler(rotation));
        } else if (op.op === 'scale') {
          const sx = op.x ?? 1;
          const sy = op.y ?? sx;
          const sz = op.z ?? sx;
          transform.multiply(new THREE.Matrix4().makeScale(sx, sy, sz));
        }
      }

      const pos = new THREE.Vector3();
      const quat = new THREE.Quaternion();
      const scl = new THREE.Vector3();
      transform.decompose(pos, quat, scl);
      text.position.copy(pos);
      text.quaternion.copy(quat);
      text.scale.copy(scl);
    } else {
      // Backward compatibility for labels produced without op lists.
      text.position.set(label.x, label.y, label.z);
      text.rotation.x = Math.PI / 2;
    }
    
    text.sync();
    labels_group.add(text);
  }
  
  console.log(`Updated ${labels.length} text labels`);
}

// Status HUD (no GPU needed)
const status_link_el = document.getElementById('status-link') as HTMLAnchorElement | null;
const status_el = document.getElementById('status')!;
const status_detail_el = document.getElementById('status-detail')!;
let last_update_time: Date | null = null;
let last_edge_count = 0;
let last_render_ms = 0;

// Server info (file, branch, port)
let server_info: { file: string; branch: string; port: number } | null = null;
const initial_query = new URLSearchParams(window.location.search);
const current_backend_id = initial_query.get('backend_id') || '';
const configured_renderer_id = (import.meta.env.VITE_MITTENS_RENDERER_ID as string | undefined)?.trim();
const configured_renderer_branch = (import.meta.env.VITE_MITTENS_RENDERER_BRANCH as string | undefined)?.trim();

function current_branch_label(): string {
  if (configured_renderer_branch && configured_renderer_branch.length > 0) {
    return configured_renderer_branch;
  }
  return server_info?.branch || 'unknown';
}

function update_status_menu_link() {
  if (!status_link_el) return;
  const params = new URLSearchParams();
  if (current_backend_id) {
    params.set('focus_backend', current_backend_id);
  }
  if (configured_renderer_id && configured_renderer_id.length > 0) {
    params.set('renderer_id', configured_renderer_id);
  }
  const suffix = params.toString();
  status_link_el.href = suffix.length > 0 ? `/graph?${suffix}` : '/graph';
}

function format_time(date: Date): string {
  return date.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit' });
}

function update_status_display() {
  if (last_update_time && server_info) {
    const filename = server_info.file.split('/').pop() || server_info.file;
    status_el.textContent = `📁 ${filename} · 🌿 ${current_branch_label()} · ${format_time(last_update_time)}`;
    status_el.style.color = '#0f0';
    status_detail_el.textContent = `${last_edge_count.toLocaleString()} edges · ${last_render_ms.toFixed(1)}ms`;
  } else if (last_update_time) {
    status_el.textContent = `Updated ${format_time(last_update_time)}`;
    status_detail_el.textContent = `${last_edge_count.toLocaleString()} edges · ${last_render_ms.toFixed(1)}ms`;
  }
}

function update_status(state: 'connecting' | 'connected' | 'disconnected' | 'error', detail?: string) {
  const icons: Record<string, string> = {
    connecting: '⏳',
    connected: '🟢',
    disconnected: '🔴',
    error: '❌',
  };
  if (state === 'connected' && server_info) {
    // Show file · branch in green
    const filename = server_info.file.split('/').pop() || server_info.file;
    status_el.textContent = `📁 ${filename} · 🌿 ${current_branch_label()}`;
    status_el.style.color = '#0f0';
  } else if (state !== 'connected' || !last_update_time) {
    status_el.textContent = `${icons[state]} ${state.toUpperCase()}`;
    status_el.style.color = '';  // Reset to default
    if (detail) {
      status_detail_el.textContent = detail;
    }
  } else {
    update_status_display();
  }
}

// WebSocket
function connect_websocket() {
  const backend_id = current_backend_id;
  const ws_scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
  if (!backend_id) {
    update_status('error', 'Missing backend_id query param');
    console.error('backend_id query param is required, e.g. ?backend_id=a1');
    return;
  }
  const ws_query = new URLSearchParams();
  if (configured_renderer_id && configured_renderer_id.length > 0) {
    ws_query.set('renderer_id', configured_renderer_id);
  }
  const ws_query_suffix = ws_query.toString();
  const ws_path = ws_query_suffix.length > 0
    ? `/ws/${encodeURIComponent(backend_id)}?${ws_query_suffix}`
    : `/ws/${encodeURIComponent(backend_id)}`;

  const ws = new WebSocket(`${ws_scheme}://${window.location.host}${ws_path}`);
  current_ws = ws;
  ws.binaryType = 'arraybuffer';

  ws.onopen = () => {
    console.log('WebSocket connected');
    update_status('connected', 'Waiting for data...');
  };

  ws.onmessage = (event) => {
    if (event.data instanceof ArrayBuffer) {
      if (try_handle_export_packet(event.data)) return;
      const start = performance.now();
      update_mesh(event.data);
      last_render_ms = performance.now() - start;
      last_update_time = new Date();
      update_status_display();
    } else if (typeof event.data === 'string') {
      // JSON message - could be labels, info, or other metadata
      try {
        const msg = JSON.parse(event.data);
        if (msg.type === 'labels' && Array.isArray(msg.labels)) {
          update_labels(msg.labels);
        } else if (msg.type === 'info') {
          // Store server info and update status display
          server_info = {
            file: msg.file || 'unknown',
            branch: msg.branch || 'unknown',
            port: msg.port || 0
          };
          current_export_files = Array.isArray(msg.exports)
            ? msg.exports.filter((v: unknown) => typeof v === 'string')
            : [];
          if (download_stl_btn) {
            download_stl_btn.textContent = current_export_files.length > 1 ? 'ZIP' : 'STL';
          }
          refresh_download_button_state();
          update_status('connected');
        } else if (msg.type === 'exports' && Array.isArray(msg.files)) {
          current_export_files = msg.files.filter((v: unknown) => typeof v === 'string');
          if (download_stl_btn) {
            download_stl_btn.textContent = current_export_files.length > 1 ? 'ZIP' : 'STL';
          }
          refresh_download_button_state();
        }
      } catch (e) {
        console.warn('Failed to parse JSON message:', e);
      }
    }
  };

  ws.onclose = () => {
    console.log('Disconnected, reconnecting...');
    update_status('disconnected', 'Reconnecting in 2s...');
    if (current_ws === ws) {
      current_ws = null;
    }
    setTimeout(connect_websocket, 2000);
  };

  ws.onerror = (err) => {
    console.error('WebSocket error:', err);
    update_status('error', 'WebSocket error');
  };
}

// Resize
function resize() {
  const w = container.clientWidth;
  const h = container.clientHeight;
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
  renderer.setSize(w, h);
}
resize();
window.addEventListener('resize', resize);

// Update circuit overlay position and scale based on 3D anchor
function update_circuit_position() {
  if (!circuit_visible) return;

  // Project 3D anchor point to screen coordinates
  const projected = circuit_anchor.clone().project(camera);

  // Convert from normalized device coords to screen pixels
  const w = container.clientWidth;
  const h = container.clientHeight;
  const x = (projected.x * 0.5 + 0.5) * w;
  const y = (-projected.y * 0.5 + 0.5) * h;

  // Calculate scale based on camera distance to orbit target (zoom-only, not rotation)
  const zoom_distance = camera.position.distanceTo(controls.target);
  const scale = Math.max(0.5, Math.min(3.0, 160 / zoom_distance));

  // Position circuit to the LEFT of the anchor (right edge at anchor)
  circuit_overlay.style.left = `${x - circuit_width * scale - 20}px`;
  circuit_overlay.style.top = `${y}px`;
  circuit_overlay.style.transform = `scale(${scale})`;
}

// Render loop
function animate() {
  requestAnimationFrame(animate);
  controls.update();
  update_circuit_position();
  renderer.render(scene, camera);
}

// Camera position display
const camera_info = document.getElementById('camera-info')!;
const camera_pos_el = document.getElementById('camera-pos')!;
const camera_target_el = document.getElementById('camera-target')!;

function update_camera_display() {
  const p = camera.position;
  const t = controls.target;
  camera_pos_el.textContent = `[${p.x.toFixed(0)}, ${p.y.toFixed(0)}, ${p.z.toFixed(0)}]`;
  camera_target_el.textContent = `[${t.x.toFixed(0)}, ${t.y.toFixed(0)}, ${t.z.toFixed(0)}]`;
}

camera_info.addEventListener('click', () => {
  const p = camera.position;
  const t = controls.target;
  const config = `camera = {
  position = {${p.x.toFixed(0)}, ${p.y.toFixed(0)}, ${p.z.toFixed(0)}},
  target = {${t.x.toFixed(0)}, ${t.y.toFixed(0)}, ${t.z.toFixed(0)}},
  up = {0, 0, 1}
}`;

  function showSuccess() {
    camera_info.style.color = '#0a8';
    setTimeout(() => camera_info.style.color = '', 500);
  }
  
  function showFailure() {
    camera_info.style.color = '#a00';
    setTimeout(() => camera_info.style.color = '', 500);
    console.warn('Clipboard copy failed');
  }

  // Mobile-friendly fallback copy
  function fallbackCopy(text: string): boolean {
    const textarea = document.createElement('textarea');
    textarea.value = text;
    // iOS requires the element to be visible and in viewport
    textarea.style.position = 'fixed';
    textarea.style.left = '0';
    textarea.style.top = '0';
    textarea.style.opacity = '0';
    textarea.style.pointerEvents = 'none';
    document.body.appendChild(textarea);
    textarea.focus();
    // iOS Safari needs setSelectionRange
    textarea.setSelectionRange(0, text.length);
    let success = false;
    try {
      success = document.execCommand('copy');
    } catch (e) {
      console.warn('execCommand copy failed:', e);
    }
    document.body.removeChild(textarea);
    return success;
  }

  // Try modern API first, then fallback
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(config).then(showSuccess).catch(() => {
      if (fallbackCopy(config)) showSuccess();
      else showFailure();
    });
  } else {
    if (fallbackCopy(config)) showSuccess();
    else showFailure();
  }
});

// Update camera display on control changes
controls.addEventListener('change', update_camera_display);
update_camera_display();

// Home button - reset camera to Lua home or default
const home_btn = document.getElementById('home-btn');
function reset_camera() {
  // Use Lua-defined camera as home, or fall back to default
  if (home_camera) {
    camera.position.set(home_camera.pos[0], home_camera.pos[1], home_camera.pos[2]);
    controls.target.set(home_camera.target[0], home_camera.target[1], home_camera.target[2]);
    camera.fov = home_camera.fov;
    camera.near = home_camera.near;
    camera.far = home_camera.far;
  } else {
    camera.position.copy(DEFAULT_CAMERA.position);
    controls.target.copy(DEFAULT_CAMERA.target);
    camera.fov = DEFAULT_CAMERA.fov;
    camera.near = 0.1;
    camera.far = 100000;
  }
  camera.updateProjectionMatrix();
  controls.update();
  localStorage.removeItem('mittens_camera');
  update_camera_display();
  console.log('Camera reset to home position');
  // Visual feedback
  if (home_btn) {
    home_btn.style.color = '#0a8';
    setTimeout(() => home_btn.style.color = '', 300);
  }
}

home_btn?.addEventListener('click', reset_camera);
download_stl_btn?.addEventListener('click', () => {
  if (current_export_files.length > 0) {
    request_export_download();
    return;
  }
  download_current_mesh_stl();
});
set_download_button_enabled(false);

function is_typing_context(): boolean {
  const active = document.activeElement;
  if (!active) return false;
  const tag = active.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  return Boolean((active as HTMLElement).isContentEditable);
}

function open_graph_menu() {
  const target = status_link_el?.href || '/graph';
  window.location.href = target;
}

// Keyboard shortcuts: Ctrl/Cmd+K for graph, H for home
document.addEventListener('keydown', (e) => {
  if (is_typing_context()) return;
  const key = e.key.toLowerCase();

  if ((e.ctrlKey || e.metaKey) && key === 'k') {
    e.preventDefault();
    open_graph_menu();
    return;
  }

  if (!e.ctrlKey && !e.metaKey && key === 'h') {
    reset_camera();
  }
});

// Start
connect_websocket();
update_status_menu_link();
animate();
