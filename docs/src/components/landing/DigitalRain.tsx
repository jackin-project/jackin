// docs/components/landing/DigitalRain.tsx
import { useEffect, useRef } from 'react';
import { createRainState, tickRain, ageToColor, type RainTheme } from './rainEngine';

export interface DigitalRainProps {
  fontSize?: number;
  cellW?: number;
  cellH?: number;
  frameMs?: number;
  opacity?: number;
}

function readTheme(): RainTheme {
  if (typeof document === 'undefined') return 'dark';
  return document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
}

export function DigitalRain({ fontSize = 14, cellW = 12, cellH = 18, frameMs = 35, opacity = 0.32 }: DigitalRainProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Theme read via ref (not state) so the render loop picks up the new
  // value without React re-rendering the component — the canvas + rAF
  // loop are long-lived and we only want colour selection to shift.
  const themeRef = useRef<RainTheme>('dark');

  useEffect(() => {
    themeRef.current = readTheme();

    const obs = new MutationObserver(() => {
      themeRef.current = readTheme();
    });
    obs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });

    return () => obs.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      // Render a single populated still frame and stop — previously
      // this branch only sized the canvas and returned, leaving
      // reduced-motion users with a blank area instead of the intended
      // static phosphor-rain backdrop. Colour palette follows the
      // active theme at the moment the still is painted; it won't
      // update if the user later toggles, but that's a narrow edge
      // case for a reduced-motion still-frame.
      const rect = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const cols = Math.max(1, Math.floor(rect.width / cellW));
      const rows = Math.max(1, Math.floor(rect.height / cellH));
      const stillState = createRainState(cols, rows);
      // ~90 ticks is well past the engine's warm-up — columns have
      // advanced through a full cycle so the frame looks natural
      // instead of sparse/top-biased.
      for (let i = 0; i < 90; i++) tickRain(stillState);
      ctx.clearRect(0, 0, rect.width, rect.height);
      ctx.font = fontSize + 'px "JetBrains Mono", "SF Mono", monospace';
      ctx.textBaseline = 'top';
      const theme = themeRef.current;
      for (let r = 0; r < stillState.rows; r++) {
        for (let c = 0; c < stillState.cols; c++) {
          const cell = stillState.grid[r][c];
          if (!cell) continue;
          const color = ageToColor(cell.age, theme);
          if (!color) continue;
          ctx.fillStyle = color;
          ctx.fillText(cell.ch, c * cellW, r * cellH);
        }
      }
      return;
    }

    let state = createRainState(Math.floor(canvas.clientWidth / cellW), Math.floor(canvas.clientHeight / cellH));
    let lastFrame = 0;
    let raf = 0;

    function resize() {
      if (!canvas) return;
      const dpr = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
      state = createRainState(Math.max(1, Math.floor(rect.width / cellW)), Math.max(1, Math.floor(rect.height / cellH)));
    }
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    function loop(ts: number) {
      raf = requestAnimationFrame(loop);
      if (ts - lastFrame < frameMs) return;
      lastFrame = ts;
      tickRain(state);
      ctx!.clearRect(0, 0, canvas!.clientWidth, canvas!.clientHeight);
      ctx!.font = fontSize + 'px "JetBrains Mono", "SF Mono", monospace';
      ctx!.textBaseline = 'top';
      const theme = themeRef.current;
      for (let r = 0; r < state.rows; r++) {
        for (let c = 0; c < state.cols; c++) {
          const cell = state.grid[r][c];
          if (!cell) continue;
          const color = ageToColor(cell.age, theme);
          if (!color) continue;
          ctx!.fillStyle = color;
          ctx!.fillText(cell.ch, c * cellW, r * cellH);
        }
      }
    }
    raf = requestAnimationFrame(loop);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [fontSize, cellW, cellH, frameMs]);

  return <canvas ref={canvasRef} className="landing-rain-canvas" style={{ opacity }} />;
}
