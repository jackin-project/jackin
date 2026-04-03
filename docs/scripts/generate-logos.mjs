import { Resvg } from '@resvg/resvg-js';
import { writeFileSync } from 'fs';

const rainLines = [
  '│ │╷│ │╷│ ╷  │╷│ │╷│ │╷│',
  '│ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│',
  '╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵',
  '           ╵',
];

function buildSvg({ width, height, rainColor, titleColor, fontSize }) {
  const lineH = fontSize * 1.35;
  const rainBlockH = rainLines.length * lineH;
  const rainX = 16;
  const rainStartY = (height - rainBlockH) / 2 + fontSize;

  const titleX = width * 0.73;
  const titleY = height / 2 + fontSize * 0.55;

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
  <style>
    .rain { font-family: 'DejaVu Sans Mono', monospace; font-size: ${fontSize}px; fill: ${rainColor}; }
    .title { font-family: 'DejaVu Sans Mono', monospace; font-size: ${fontSize * 1.7}px; fill: ${titleColor}; font-weight: bold; letter-spacing: 0.25em; }
  </style>
  ${rainLines.map((line, i) => `<text class="rain" x="${rainX}" y="${rainStartY + i * lineH}">${line}</text>`).join('\n  ')}
  <text class="title" x="${titleX}" y="${titleY}" text-anchor="middle">j a c k i n</text>
</svg>`;
}

function svgToPng(svg, scale) {
  const resvg = new Resvg(svg, {
    fitTo: { mode: 'zoom', value: scale },
    font: { loadSystemFonts: true },
  });
  return resvg.render().asPng();
}

// Navbar logos (2x for retina)
const navDark = buildSvg({ width: 460, height: 80, rainColor: '#5f87ff', titleColor: '#ffffff', fontSize: 13 });
const navLight = buildSvg({ width: 460, height: 80, rainColor: '#3a6fbf', titleColor: '#1a1a1a', fontSize: 13 });
writeFileSync('src/assets/logo-dark.png', svgToPng(navDark, 2));
writeFileSync('src/assets/logo-light.png', svgToPng(navLight, 2));
console.log('Navbar logos done');

// Hero logos (2x for retina)
const heroDark = buildSvg({ width: 680, height: 130, rainColor: '#5f87ff', titleColor: '#ffffff', fontSize: 20 });
const heroLight = buildSvg({ width: 680, height: 130, rainColor: '#3a6fbf', titleColor: '#1a1a1a', fontSize: 20 });
writeFileSync('src/assets/hero-logo-dark.png', svgToPng(heroDark, 2));
writeFileSync('src/assets/hero-logo-light.png', svgToPng(heroLight, 2));
console.log('Hero logos done');
