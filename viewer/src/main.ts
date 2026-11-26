// chunkfetti viewer: streams a chunkfetti tileset (3D Tiles 1.1) and lets you fly from the whole world down to a
// single block. Tiles near the camera are swapped for sharper ones; far ones stay coarse or are dropped.
//
// URL parameters:
//   ?tiles=<url of tileset.json>   default: tiles/tileset.json next to this page
//   &detail=<n>                    screen-space error target in pixels (lower = sharper, heavier); default 4
//   &cut=<y>                       start with the world cut away above block height y (the underground view)
import * as THREE from "three/webgpu";
import { TilesRenderer } from "3d-tiles-renderer";
import { MapControls } from "three/examples/jsm/controls/MapControls.js";

const params = new URLSearchParams(location.search);
const url = params.get("tiles") ?? "tiles/tileset.json";
const hud = document.getElementById("hud")!;

async function main() {
  const renderer = new THREE.WebGPURenderer({ antialias: true }); // WebGL 2 where WebGPU is missing
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.setSize(innerWidth, innerHeight);
  document.body.prepend(renderer.domElement);
  await renderer.init();

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x9cc3e6);
  scene.fog = new THREE.Fog(0x9cc3e6, 4000, 30000);
  const camera = new THREE.PerspectiveCamera(55, innerWidth / innerHeight, 1, 200000);

  // the tileset's frame is z-up; turning it back makes the scene use the game's own axes (y up)
  const tiles = new TilesRenderer(url);
  tiles.errorTarget = Number(params.get("detail") ?? 4);
  tiles.group.rotation.x = -Math.PI / 2;

  // underground view: everything above the cut height is clipped away. Solid ground has no faces inside it, so
  // once the top is cut off you look straight down into tunnels, stations and basements.
  const clip = new THREE.ClippingGroup();
  const plane = new THREE.Plane(new THREE.Vector3(0, -1, 0), 60);
  clip.clippingPlanes = [plane];
  clip.add(tiles.group);
  scene.add(clip);
  const sky = new THREE.Color(0x9cc3e6);
  const earth = new THREE.Color(0x2b2118);
  const cut = setupCut(plane, clip, (on) => {
    scene.background = on ? earth : sky; // underground: look into dark earth, not sky
    scene.fog = on ? null : new THREE.Fog(0x9cc3e6, 4000, 30000);
  });
  tiles.setCamera(camera);
  tiles.setResolutionFromRenderer(camera, renderer as never);

  const controls = new MapControls(camera, renderer.domElement);
  controls.screenSpacePanning = false;
  controls.zoomToCursor = true;
  controls.maxPolarAngle = Math.PI * 0.495;

  let framed = false;
  tiles.addEventListener("load-tileset", () => {
    if (framed) return;
    const sphere = new THREE.Sphere();
    tiles.group.updateMatrixWorld(true);
    const box = new THREE.Box3();
    if (tiles.getBoundingBox(box)) cut.range(box.min.y, box.max.y);
    if (tiles.getBoundingSphere(sphere)) {
      framed = true;
      const c = sphere.center;
      camera.position.set(c.x, c.y + sphere.radius * 1.2, c.z + sphere.radius * 0.9);
      controls.target.copy(c);
      controls.update();
    }
  });

  addEventListener("resize", () => {
    camera.aspect = innerWidth / innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(innerWidth, innerHeight);
    tiles.setResolutionFromRenderer(camera, renderer as never);
  });

  let last = 0;
  renderer.setAnimationLoop((t: number) => {
    controls.update();
    camera.updateMatrixWorld();
    tiles.update();
    renderer.render(scene, camera);
    if (t - last > 500) {
      last = t;
      const p = controls.target;
      hud.textContent = `x ${p.x.toFixed(0)}  z ${p.z.toFixed(0)}  height ${camera.position.y.toFixed(0)}   tiles on screen ${tiles.visibleTiles.size}`;
    }
  });
}

/** The cut-away panel: a toggle (U) and a height slider ([ and ] keys). */
function setupCut(plane: THREE.Plane, clip: THREE.ClippingGroup, changed: (on: boolean) => void) {
  const panel = document.getElementById("cut")!;
  const toggle = document.getElementById("cut-toggle") as HTMLButtonElement;
  const slider = document.getElementById("cut-y") as HTMLInputElement;
  const label = document.getElementById("cut-label")!;
  const set = (on: boolean, y = Number(slider.value)) => {
    slider.value = String(Math.round(y));
    plane.constant = Number(slider.value) + 0.01; // keeps the blocks at the cut height, removes everything above
    clip.enabled = on;
    panel.dataset.on = String(on);
    toggle.setAttribute("aria-pressed", String(on));
    toggle.textContent = on ? "Underground: on" : "Underground: off";
    label.textContent = `y ${slider.value}`;
    changed(on);
  };
  toggle.addEventListener("click", () => set(clip.enabled === false));
  slider.addEventListener("input", () => set(true));
  addEventListener("keydown", (e) => {
    if (e.target instanceof HTMLInputElement && e.key !== "[" && e.key !== "]") return;
    if (e.key === "u" || e.key === "U") set(!clip.enabled);
    if (e.key === "[") set(true, Number(slider.value) - (e.shiftKey ? 10 : 1));
    if (e.key === "]") set(true, Number(slider.value) + (e.shiftKey ? 10 : 1));
  });
  const start = params.get("cut");
  set(start !== null, start !== null ? Number(start) : Number(slider.value));
  return {
    range(lo: number, hi: number) {
      slider.min = String(Math.floor(lo));
      slider.max = String(Math.ceil(hi));
      set(clip.enabled);
    },
  };
}

main().catch((e) => {
  hud.textContent = `chunkfetti viewer could not start: ${e}`;
});
