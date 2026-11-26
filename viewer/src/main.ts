// chunkfetti viewer: streams a chunkfetti tileset (3D Tiles 1.1) and lets you fly from the whole world down to a
// single block. Tiles near the camera are swapped for sharper ones; far ones stay coarse or are dropped.
//
// URL parameters:
//   ?tiles=<url of tileset.json>   default: tiles/tileset.json next to this page
//   &detail=<n>                    screen-space error target in pixels (lower = sharper, heavier); default 4
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
  scene.add(tiles.group);
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

main().catch((e) => {
  hud.textContent = `chunkfetti viewer could not start: ${e}`;
});
