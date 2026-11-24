<template>
  <div class="flow-background">
    <canvas ref="canvasRef"></canvas>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';

const canvasRef = ref<HTMLCanvasElement | null>(null);
let animationId = 0;

onMounted(() => {
  const canvas = canvasRef.value;
  if (!canvas) return;

  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  let width = 0;
  let height = 0;
  let time = 0;

  const handleResize = () => {
    width = window.innerWidth;
    height = window.innerHeight;
    canvas.width = width;
    canvas.height = height;
  };

  handleResize();
  window.addEventListener('resize', handleResize);

  const particles = Array.from({ length: 90 }, () => ({
    x: Math.random() * width,
    y: Math.random() * height,
    vx: (Math.random() - 0.5) * 0.3,
    vy: (Math.random() - 0.5) * 0.3,
    size: Math.random() * 1.5 + 0.5,
    alpha: Math.random() * 0.3 + 0.06,
  }));

  /**
   * Draw the central wave layer
   */
  const drawWaves = () => {
    const isDark = document.documentElement.getAttribute('data-theme') !== 'light';
    // Navy main wave, bronze and rust accents
    const colors = isDark
      ? ['140, 162, 228', '192, 176, 134', '160, 175, 215', '214, 110, 88']
      : ['18, 29, 62', '132, 118, 82', '18, 29, 62', '176, 59, 40'];
    const alphas = isDark ? [0.2, 0.1, 0.06, 0.06] : [0.14, 0.12, 0.05, 0.06];

    const lines = 4;
    for (let i = 0; i < lines; i++) {
      ctx.beginPath();

      ctx.lineWidth = i === 0 ? 1.6 : 1;
      ctx.strokeStyle = `rgba(${colors[i]}, ${alphas[i]})`;

      for (let x = 0; x <= width; x += 10) {
        const yOffset = height / 2;

        // Vary the frequencies so the waves move independently
        const y1 = Math.sin((x * 0.003) + time + i * 1.5) * (height * 0.2);
        const y2 = Math.sin((x * 0.006) - time * 1.2 + i * 0.8) * (height * 0.15);
        const y3 = Math.sin((x * 0.01) + time * 0.8 + Math.PI * i) * (height * 0.05);

        const y = yOffset + y1 + y2 + y3;

        if (x === 0) {
          ctx.moveTo(x, y);
        } else {
          ctx.lineTo(x, y);
        }
      }

      ctx.stroke();
    }
  };

  /**
   * Wrap drifting particles to the opposite edge when they leave the canvas
   */
  const drawParticles = () => {
    const isDark = document.documentElement.getAttribute('data-theme') !== 'light';
    const dotColor = isDark ? '200, 210, 240' : '18, 29, 62';

    particles.forEach((particle) => {
      particle.x += particle.vx;
      particle.y += particle.vy - 0.1;

      if (particle.x < 0) particle.x = width;
      if (particle.x > width) particle.x = 0;
      if (particle.y < 0) particle.y = height;
      if (particle.y > height) particle.y = 0;

      ctx.beginPath();
      ctx.arc(particle.x, particle.y, particle.size, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(${dotColor}, ${particle.alpha})`;
      ctx.fill();
    });
  };

  /**
   * Advance and redraw the background once per animation frame
   */
  const render = () => {
    time += 0.002;
    ctx.clearRect(0, 0, width, height);

    drawWaves();
    drawParticles();

    if (!reducedMotion) animationId = requestAnimationFrame(render);
  };

  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  // Resizing clears the canvas, so redraw the static frame
  if (reducedMotion) window.addEventListener('resize', render);

  render();

  onBeforeUnmount(() => {
    window.removeEventListener('resize', handleResize);
    window.removeEventListener('resize', render);
    cancelAnimationFrame(animationId);
  });
});
</script>

<style scoped>
.flow-background {
  position: absolute;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  z-index: 0;
  pointer-events: none;
  overflow: hidden;
}

canvas {
  width: 100%;
  height: 100%;
}
</style>
