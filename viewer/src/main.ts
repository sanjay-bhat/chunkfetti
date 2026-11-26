// chunkfetti viewer, first cut: a burst of block confetti to prove the renderer runs.
// Tile streaming (orbit-to-street level of detail) arrives in the next releases.
import * as THREE from "three/webgpu";

const COLOURS = [0x5d9c3a, 0x866043, 0x7f7f7f, 0x3f76e4, 0xe0c16a, 0xd96f3c, 0xb7d3e6, 0x8e6cc4];

async function main() {
  const renderer = new THREE.WebGPURenderer({ antialias: true }); // falls back to WebGL 2 where WebGPU is missing
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.setSize(innerWidth, innerHeight);
  document.body.prepend(renderer.domElement);
  await renderer.init();

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x14122b);
  const camera = new THREE.PerspectiveCamera(55, innerWidth / innerHeight, 0.1, 500);
  camera.position.set(0, 4, 26);
  scene.add(new THREE.HemisphereLight(0xffffff, 0x333355, 2.2));

  const box = new THREE.BoxGeometry(1, 1, 1);
  const count = 400;
  const mesh = new THREE.InstancedMesh(box, new THREE.MeshLambertMaterial(), count);
  const m = new THREE.Matrix4();
  for (let i = 0; i < count; i++) {
    const r = 3 + Math.random() * 12;
    const a = Math.random() * Math.PI * 2;
    const y = (Math.random() - 0.3) * 14;
    m.makeTranslation(Math.cos(a) * r, y, Math.sin(a) * r);
    mesh.setMatrixAt(i, m);
    mesh.setColorAt(i, new THREE.Color(COLOURS[i % COLOURS.length]));
  }
  scene.add(mesh);

  addEventListener("resize", () => {
    camera.aspect = innerWidth / innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(innerWidth, innerHeight);
  });
  renderer.setAnimationLoop((t: number) => {
    mesh.rotation.y = t * 0.0001;
    renderer.render(scene, camera);
  });
}

main().catch((e) => {
  document.getElementById("hud")!.textContent = `chunkfetti viewer could not start: ${e}`;
});
