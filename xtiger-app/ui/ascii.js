'use strict';

/* The ASCII water field behind the home screen.
 *
 * A picture is turned into a grid of letters and digits. Each cell picks its character from a ramp
 * ordered by how much ink it carries, so brighter parts of the picture get denser letters and the
 * shading reads smoothly instead of as noise. A height field runs the wave equation on the same grid:
 * the pointer pushes it, and the picture is read through the bent surface (with bilinear sampling, so
 * nothing jumps from cell to cell), which makes the scene move like it is seen through water.
 * Nothing here talks to the Rust side. */

const AsciiField = (() => {
  // Lightest to heaviest: the more ink a letter has, the brighter the part of the picture it draws.
  // Each tier mixes a few letters of similar weight, so the picture stays lively but the shading is smooth.
  const TIERS = ['ilrtj1', 'cvxznuso', 'eaykhdbpq', 'gmwOZ0S', 'EKB8WMQ'];
  const FONT_PX = 12;
  const CELL_W = 7.4;
  const CELL_H = 14;
  const STEP_MS = 1000 / 60;
  const LEVELS = 12;
  const DAMPING = 0.986;
  const BEND = 7;
  const HOT_FROM = 0.6;
  // The part of the picture that is drawn: Qubis and the tiger. The whole scene is too busy to read as
  // letters, so the sky and the trees are left out.
  const CROP = { x: 0.04, y: 0.26, w: 0.82, h: 0.72, aspect: 0.82 / 0.72 };
  // How many cells the text column of the home screen takes from the left.
  const TEXT_COLS = 84;

  const reducedMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const clamp = (value, low, high) => Math.min(high, Math.max(low, value));

  /* The picture as a grid of 0..1 values: brightness plus some edges, softly blurred so the shading
   * flows. */
  function sample(image, cols, rows) {
    const scratch = document.createElement('canvas');
    scratch.width = cols;
    scratch.height = rows;
    const ctx = scratch.getContext('2d', { willReadFrequently: true });
    // The picture is a square. It fills the height and sits on the right, so the left side stays
    // quiet for the text; on a narrow window it is centered and cropped instead. Its left part (just
    // a tree) is cut off so the tiger and the castle get the room.
    const full = rows * (CELL_H / CELL_W) * CROP.aspect;
    // On a wide window the picture fits whole in the space the text leaves free; on a narrow one it
    // sits behind the text, faint, centered and cropped.
    const free = cols - TEXT_COLS;
    const roomy = free >= 36;
    const side = roomy ? Math.min(full * 0.92, free - 4) : full;
    const high = (side * CELL_W) / CELL_H / CROP.aspect;
    const left = roomy ? cols - side - 2 : side >= cols ? (cols - side) / 2 : cols - side;
    const top = roomy ? (rows - high) / 2 : 0;
    ctx.imageSmoothingEnabled = true;
    ctx.drawImage(
      image,
      image.width * CROP.x, image.height * CROP.y, image.width * CROP.w, image.height * CROP.h,
      left, top, side, roomy ? high : rows,
    );
    const { data } = ctx.getImageData(0, 0, cols, rows);
    const lum = new Float32Array(cols * rows);
    for (let i = 0; i < lum.length; i += 1) {
      lum[i] = (0.2126 * data[i * 4] + 0.7152 * data[i * 4 + 1] + 0.0722 * data[i * 4 + 2]) / 255;
    }
    const raw = new Float32Array(cols * rows);
    for (let y = 0; y < rows; y += 1) {
      for (let x = 0; x < cols; x += 1) {
        const i = y * cols + x;
        const covered = data[i * 4 + 3] / 255;
        const at = (dx, dy) => lum[clamp(y + dy, 0, rows - 1) * cols + clamp(x + dx, 0, cols - 1)];
        const gx = at(1, 0) - at(-1, 0);
        const gy = at(0, 1) - at(0, -1);
        const edge = Math.min(1, Math.hypot(gx, gy) * 1.4);
        // The sky and the hills are cool and bright, so brightness alone would fill the whole square.
        // The warm colors (the tiger, the path, the roofs) and the dark shapes (Qubis, the stripes)
        // are what make the picture, so those are what the letters follow.
        const warm = clamp((data[i * 4] - data[i * 4 + 2]) / 140, 0, 1);
        const dark = clamp((0.3 - lum[i]) / 0.2, 0, 1);
        const tone = Math.max(0.55 * warm, dark);
        // The picture melts into the empty space around it instead of ending on a hard edge.
        const near = roomy
          ? Math.min(x - left, left + side - x, y - top, top + high - y) / (side * 0.16)
          : (x - left) / (side * 0.4);
        const fade = clamp(near, 0, 1);
        raw[i] = clamp((0.95 * tone + 0.6 * edge) * covered * fade * fade * (3 - 2 * fade), 0, 1);
      }
    }
    // One pass of a 3x3 blur takes the grain out.
    const base = new Float32Array(cols * rows);
    for (let y = 0; y < rows; y += 1) {
      for (let x = 0; x < cols; x += 1) {
        let sum = 0;
        let weight = 0;
        for (let dy = -1; dy <= 1; dy += 1) {
          for (let dx = -1; dx <= 1; dx += 1) {
            const w = dx === 0 && dy === 0 ? 4 : dx === 0 || dy === 0 ? 2 : 1;
            sum += w * raw[clamp(y + dy, 0, rows - 1) * cols + clamp(x + dx, 0, cols - 1)];
            weight += w;
          }
        }
        base[y * cols + x] = sum / weight;
      }
    }
    return base;
  }

  function colors() {
    const style = getComputedStyle(document.documentElement);
    const read = (name, fallback) => style.getPropertyValue(name).trim() || fallback;
    return { ink: read('--ascii-ink', '#6F82AB'), hot: read('--ascii-hot', '#8FB6FF') };
  }

  function mount(host, { src }) {
    const canvas = document.createElement('canvas');
    canvas.className = 'ascii-canvas';
    canvas.setAttribute('aria-hidden', 'true');
    host.prepend(canvas);
    const ctx = canvas.getContext('2d');

    const still = reducedMotion();
    let alive = true;
    let frame = 0;
    let last = 0;
    let owed = 0;
    let cols = 0;
    let rows = 0;
    let base = null;
    let wobble = null;
    let height = null;
    let previous = null;
    let bucket = null;
    let pick = null;
    let order = null;
    let image = null;
    let palette = colors();
    let nextDrop = 0;
    let wide = 0;
    let tall = 0;

    const build = () => {
      const box = host.getBoundingClientRect();
      wide = Math.max(1, Math.round(box.width));
      tall = Math.max(1, Math.round(box.height));
      const ratio = Math.min(2, window.devicePixelRatio || 1);
      canvas.width = Math.round(wide * ratio);
      canvas.height = Math.round(tall * ratio);
      ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
      cols = Math.ceil(wide / CELL_W);
      rows = Math.ceil(tall / CELL_H);
      const cells = cols * rows;
      // A fixed random number per cell picks its letter inside the tier, so neighbours with the same
      // brightness do not all draw the same letter. It never changes, which keeps the picture calm.
      wobble = Float32Array.from({ length: cells }, () => Math.random());
      height = new Float32Array(cells);
      previous = new Float32Array(cells);
      bucket = new Uint8Array(cells);
      pick = new Uint8Array(cells);
      order = new Uint32Array(cells);
      base = image ? sample(image, cols, rows) : null;
      paint(performance.now());
    };

    /* One step of the wave equation. */
    const step = () => {
      for (let y = 1; y < rows - 1; y += 1) {
        for (let x = 1; x < cols - 1; x += 1) {
          const i = y * cols + x;
          const next = (height[i - 1] + height[i + 1] + height[i - cols] + height[i + cols]) * 0.5 - previous[i];
          previous[i] = next * DAMPING;
        }
      }
      [height, previous] = [previous, height];
    };

    /* A smooth bump pushed into the water. */
    const drop = (x, y, power, radius) => {
      const reach = Math.ceil(radius);
      for (let dy = -reach; dy <= reach; dy += 1) {
        for (let dx = -reach; dx <= reach; dx += 1) {
          const gx = Math.round(x) + dx;
          const gy = Math.round(y) + dy;
          if (gx < 1 || gy < 1 || gx >= cols - 1 || gy >= rows - 1) continue;
          const distance = Math.hypot(dx, dy) / radius;
          if (distance < 1) height[gy * cols + gx] -= power * 0.5 * (1 + Math.cos(Math.PI * distance));
        }
      }
    };

    const read = (x, y) => {
      const cx = clamp(x, 0, cols - 1.001);
      const cy = clamp(y, 0, rows - 1.001);
      const x0 = Math.floor(cx);
      const y0 = Math.floor(cy);
      const fx = cx - x0;
      const fy = cy - y0;
      const i = y0 * cols + x0;
      return (
        (base[i] * (1 - fx) + base[i + 1] * fx) * (1 - fy) +
        (base[i + cols] * (1 - fx) + base[i + cols + 1] * fx) * fy
      );
    };

    function paint(now) {
      if (!base) return;
      const time = now / 1000;
      ctx.clearRect(0, 0, wide, tall);
      ctx.font = `${FONT_PX}px "Geist Mono", ui-monospace, Consolas, monospace`;
      ctx.textBaseline = 'top';
      const cx = cols / 2;
      const cy = rows / 2;
      const counts = new Uint32Array(LEVELS * 2 + 2);
      for (let y = 0; y < rows; y += 1) {
        for (let x = 0; x < cols; x += 1) {
          const i = y * cols + x;
          let value;
          if (x > 0 && y > 0 && x < cols - 1 && y < rows - 1) {
            value = read(x + (height[i + 1] - height[i - 1]) * BEND, y + (height[i + cols] - height[i - cols]) * BEND);
            // Crests catch the light a little.
            value += clamp((height[i - cols - 1] - height[i + cols + 1]) * 0.55, -0.12, 0.3);
          } else {
            value = base[i];
          }
          // A very slow swell, so the picture breathes even when nothing touches it.
          if (!still) value += 0.03 * Math.sin(x * 0.12 + time * 0.5) * Math.sin(y * 0.17 - time * 0.4);
          // Soft vignette: the corners fall away.
          const ex = (x - cx) / cx;
          const ey = (y - cy) / cy;
          value *= clamp(1.15 - 0.4 * (ex * ex + ey * ey), 0.2, 1);
          if (value < 0.07) {
            bucket[i] = 0;
            continue;
          }
          const level = clamp(Math.floor(value * LEVELS), 1, LEVELS);
          bucket[i] = level + (value > HOT_FROM ? LEVELS : 0);
          pick[i] = clamp(Math.floor(value * TIERS.length), 0, TIERS.length - 1);
          counts[bucket[i]] += 1;
        }
      }
      // Counting sort, so each alpha level is drawn in one go.
      const starts = new Uint32Array(counts.length);
      for (let b = 1; b < counts.length; b += 1) starts[b] = starts[b - 1] + counts[b - 1];
      for (let i = 0; i < bucket.length; i += 1) if (bucket[i]) order[starts[bucket[i]]++] = i;
      let at = 0;
      for (let b = 1; b < counts.length; b += 1) {
        const hot = b > LEVELS;
        const strength = (hot ? b - LEVELS : b) / LEVELS;
        ctx.fillStyle = hot ? palette.hot : palette.ink;
        ctx.globalAlpha = hot ? 0.3 + 0.6 * strength : 0.08 + 0.5 * strength;
        for (let n = 0; n < counts[b]; n += 1, at += 1) {
          const i = order[at];
          const tier = TIERS[pick[i]];
          ctx.fillText(tier[Math.floor(wobble[i] * tier.length)], (i % cols) * CELL_W, Math.floor(i / cols) * CELL_H);
        }
      }
      ctx.globalAlpha = 1;
    }

    const tick = (now) => {
      if (!alive) return;
      frame = requestAnimationFrame(tick);
      if (!host.isConnected) return destroy();
      if (document.hidden) {
        last = 0;
        return;
      }
      owed += last ? Math.min(100, now - last) : STEP_MS;
      last = now;
      if (now > nextDrop) {
        drop(2 + Math.random() * (cols - 4), 2 + Math.random() * (rows - 4), 1.5, 6);
        nextDrop = now + 3500 + Math.random() * 3500;
      }
      // The water always runs at the same speed, however fast the screen refreshes.
      while (owed >= STEP_MS) {
        step();
        owed -= STEP_MS;
      }
      paint(now);
    };

    const cellAt = (event) => {
      const box = canvas.getBoundingClientRect();
      return [(event.clientX - box.left) / CELL_W, (event.clientY - box.top) / CELL_H];
    };
    let trail = null;
    const onMove = (event) => {
      const [x, y] = cellAt(event);
      if (x < 0 || y < 0 || x >= cols || y >= rows) {
        trail = null;
        return;
      }
      // Walk from the last position, so a fast pointer leaves a smooth wake instead of dots.
      const from = trail ?? [x, y];
      const length = Math.hypot(x - from[0], y - from[1]);
      const steps = Math.max(1, Math.ceil(length / 1.5));
      for (let n = 1; n <= steps; n += 1) {
        drop(from[0] + ((x - from[0]) * n) / steps, from[1] + ((y - from[1]) * n) / steps, 0.34, 3.5);
      }
      trail = [x, y];
    };
    const onDown = (event) => {
      const [x, y] = cellAt(event);
      if (x >= 0 && y >= 0 && x < cols && y < rows) drop(x, y, 3.2, 6);
    };

    function destroy() {
      alive = false;
      cancelAnimationFrame(frame);
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerdown', onDown);
      observer.disconnect();
      themeWatch.disconnect();
      canvas.remove();
    }

    // A still field has no frame loop to notice that its screen was left, so the watchers check too.
    const observer = new ResizeObserver(() => (host.isConnected ? build() : destroy()));
    observer.observe(host);
    const themeWatch = new MutationObserver(() => {
      if (!host.isConnected) return destroy();
      palette = colors();
      paint(performance.now());
    });
    themeWatch.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });

    const picture = new Image();
    picture.onload = () => {
      image = picture;
      build();
      if (!still && alive) {
        window.addEventListener('pointermove', onMove, { passive: true });
        window.addEventListener('pointerdown', onDown, { passive: true });
        frame = requestAnimationFrame(tick);
      }
    };
    picture.src = src;

    return { destroy };
  }

  return { mount };
})();
