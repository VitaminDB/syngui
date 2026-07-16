/* === Visual Effects === */

.fx-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
}

.fx-code {
    font-size: 10px;
    color: var(--text-muted);
    font-family: monospace;
}

.fx-demo-text {
    color: #ffffff;
}

.fx-glass-subtitle {
    color: rgba(255, 255, 255, 0.7);
}

/* Base demo box for filters */
.fx-demo-box {
    width: 140px;
    height: 80px;
    border-radius: 10px;
    padding: 10px;
}

/* Base demo box for shadows */
.fx-shadow-subject {
    width: 140px;
    height: 70px;
    border-radius: 10px;
    padding: 12px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
}

/* Drop Shadows */
.fx-drop-shadow-sm  { box-shadow: 0 2 8 var(--fx-shadow); }
.fx-drop-shadow-md  { box-shadow: 0 4 16 var(--fx-shadow-md); }
.fx-drop-shadow-lg  { box-shadow: 0 8 32 var(--fx-shadow-lg); }
.fx-drop-shadow-colored { box-shadow: 0 4 20 rgba(99,102,241,0.5); }

/* Inner Shadows — stronger alpha for visibility */
.fx-inner-shadow      { box-shadow: inset 0 2 8 rgba(0,0,0,0.35); }
.fx-inner-shadow-deep { box-shadow: inset 0 4 16 rgba(0,0,0,0.45); }
.fx-inner-shadow-top  { box-shadow: inset 0 -4 12 rgba(0,0,0,0.4); }
.fx-glow              { box-shadow: 0 0 20 rgba(99,102,241,0.65); }

/* Text Shadow — sharp/soft/glow */
.fx-text-shadow-sharp {
    font-size: 28px;
    font-weight: 700;
    color: var(--text);
    text-shadow: 1 1 0 rgba(0, 0, 0, 0.55);
}
.fx-text-shadow-soft {
    font-size: 28px;
    font-weight: 700;
    color: var(--text);
    text-shadow: 2 2 4 rgba(0, 0, 0, 0.6);
}
.fx-text-shadow-deep {
    font-size: 28px;
    font-weight: 700;
    color: var(--text);
    text-shadow: 3 3 6 rgba(0, 0, 0, 0.55);
}
.fx-text-shadow-glow {
    font-size: 28px;
    font-weight: 700;
    color: var(--indigo);
    text-shadow: 0 0 8 rgba(80, 200, 255, 0.95);
}

/* CSS Filters */
.fx-blur-4    { filter: blur(4px); }
.fx-blur-8    { filter: blur(8px); }
.fx-grayscale { filter: grayscale(100%); }
.fx-sepia     { filter: sepia(80%); }
.fx-invert    { filter: invert(100%); }
.fx-brightness { filter: brightness(1.5); }
.fx-contrast  { filter: contrast(2.0); }
.fx-pixelate  { filter: pixelate(8px); }
.fx-chroma    { filter: chromatic-aberration(3px); }
.fx-edge      { filter: edge-detect(0.3); }
.fx-scanlines { filter: crt(0.5); }
.fx-displacement { filter: wave(4px, 0.5); }

/* Overlay Effects */
.fx-tint-red  { color-tint: rgba(255, 60, 0, 0.35); }
.fx-tint-blue { color-tint: rgba(0, 120, 255, 0.35); }
.fx-noise     { noise: 0.35; }
.fx-vignette  { vignette: 0.7; }

/* Outline */
.fx-outline-default { outline-width: 2px; outline-color: var(--indigo); }
.fx-outline-wide    { outline-width: 4px; outline-color: var(--success); }
.fx-outline-offset  { outline-width: 2px; outline-color: var(--indigo); outline-offset: 4px; }
.fx-outline-rounded { border-radius: 16px; outline-width: 2px; outline-color: var(--warning); }

/* Filter Chains */
.fx-chain-1 { filter: blur(2px) grayscale(70%); }
.fx-chain-2 { filter: sepia(60%) vignette(0.5); }
.fx-chain-3 { filter: brightness(1.2) noise(0.2); }
.fx-chain-4 { filter: invert(100%) chromatic-aberration(2px); }

/* Glow (additive blend) */
.fx-glow-blue {
    glow: 0 0 24 rgba(99, 102, 241, 0.8);
}
.fx-glow-cyan {
    glow: 0 0 20 rgba(34, 211, 238, 0.7);
}
.fx-glow-pink {
    glow: 0 0 28 rgba(236, 72, 153, 0.75);
}
.fx-glow-green {
    glow: 0 0 22 rgba(34, 197, 94, 0.7);
}
.fx-glow-multi {
    glow: 0 0 20 rgba(99, 102, 241, 0.6), 0 0 40 rgba(236, 72, 153, 0.3);
}
.fx-glow-neon {
    glow: 0 0 8 rgba(34, 211, 238, 1.0), 0 0 24 rgba(34, 211, 238, 0.5), 0 0 48 rgba(34, 211, 238, 0.2);
    border: 1px solid rgba(34, 211, 238, 0.6);
}

/* Filter Transitions */
.fx-trans-blur {
    transition: filter 400ms ease;
    &:hover { filter: blur(6px); }
}
.fx-trans-grayscale {
    transition: filter 400ms ease;
    &:hover { filter: grayscale(100%); }
}
.fx-trans-sepia {
    transition: filter 400ms ease;
    &:hover { filter: sepia(80%); }
}
.fx-trans-bright {
    transition: filter 400ms ease;
    &:hover { filter: brightness(1.5); }
}
.fx-trans-contrast {
    transition: filter 400ms ease;
    &:hover { filter: contrast(2.0); }
}
.fx-trans-invert {
    transition: filter 400ms ease;
    &:hover { filter: invert(100%); }
}
.fx-trans-pixelate {
    transition: filter 400ms ease;
    &:hover { filter: pixelate(6px); }
}
.fx-trans-hue {
    transition: filter 400ms ease;
    &:hover { filter: hue-rotate(180deg); }
}

/* Keyframe effect animations */
.fx-anim-pulse {
    animation-name: pulse;
    animation-duration: 2s;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}
.fx-anim-breathe {
    animation-name: breathe;
    animation-duration: 3s;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}
.fx-anim-hue {
    animation-name: hue-rotate;
    animation-duration: 4s;
    animation-iteration-count: infinite;
    animation-timing-function: linear;
}
.fx-anim-glow-pulse {
    animation-name: glow-pulse;
    animation-duration: 2s;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}
.fx-anim-shadow-breathe {
    animation-name: shadow-breathe;
    animation-duration: 3s;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}
.fx-anim-color-shift {
    animation-name: color-shift;
    animation-duration: 4s;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}
.fx-anim-border-glow {
    border: 2px solid rgba(99, 102, 241, 0.3);
    animation-name: border-glow;
    animation-duration: 2s;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}
.fx-anim-float {
    animation-name: float;
    animation-duration: 3s;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}
.fx-anim-shake {
    animation-name: shake;
    animation-duration: 0.5s;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}
.fx-anim-spin-scale {
    animation-name: spin-scale;
    animation-duration: 3s;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}

/* Glassmorphism */
.fx-glass-scene {
    background: linear-gradient(135deg, #6366f1, #8b5cf6, #ec4899);
    border-radius: 16px;
    padding: 32px;
    min-height: 180px;
}

.fx-glass-card {
    background: rgba(255, 255, 255, 0.15);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 12px;
    padding: 16px;
    width: 160px;
    height: 80px;
}

.fx-glass-card-dark {
    background: rgba(0, 0, 0, 0.2);
    backdrop-filter: blur(8px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    padding: 16px;
    width: 160px;
    height: 80px;
}

.fx-glass-card-strong {
    background: rgba(255, 255, 255, 0.25);
    backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.35);
    border-radius: 12px;
    padding: 16px;
    width: 160px;
    height: 80px;
}

/* Canvas */
.canvas-card {
    border-radius: 12px;
    box-shadow: 0 4px 12px var(--shadow-color);
}

/* === Effects Showcase Sidebar === */

.effects-sidebar {
    width: 220px;
    background: var(--sidebar-bg);
    border-right: 1px solid var(--border);
}

/* Showcase demo containers */
.fx-showcase-box {
    width: 160px;
    height: 100px;
    border-radius: 12px;
    padding: 12px;
}

.fx-showcase-box-sm {
    width: 120px;
    height: 80px;
    border-radius: 10px;
    padding: 10px;
}

.fx-showcase-box-lg {
    width: 200px;
    height: 120px;
    border-radius: 14px;
    padding: 16px;
}

.fx-showcase-wide {
    width: 300px;
    height: 100px;
    border-radius: 12px;
    padding: 12px;
}

.fx-showcase-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 16px;
}

/* Opacity showcase */
.fx-opacity-100 { opacity: 1.0; }
.fx-opacity-80 { opacity: 0.8; }
.fx-opacity-60 { opacity: 0.6; }
.fx-opacity-40 { opacity: 0.4; }
.fx-opacity-20 { opacity: 0.2; }

.fx-trans-opacity {
    opacity: 1.0;
    transition: opacity 400ms ease;
    &:hover { opacity: 0.3; }
}

.fx-trans-shadow {
    box-shadow: 0 2 8 rgba(0,0,0,0.1);
    transition: box-shadow 400ms ease;
    &:hover { box-shadow: 0 8 32 rgba(0,0,0,0.3); }
}

.fx-trans-glow {
    transition: glow 400ms ease;
    &:hover { glow: 0 0 24 rgba(99, 102, 241, 0.8); }
}

.fx-trans-outline {
    outline-width: 0px;
    outline-color: var(--accent);
    transition: outline-width 300ms ease;
    &:hover { outline-width: 3px; }
}

/* Distortion demos */
.fx-pixelate-4 { filter: pixelate(4px); }
.fx-pixelate-8 { filter: pixelate(8px); }
.fx-pixelate-16 { filter: pixelate(16px); }
.fx-chroma-2 { filter: chromatic-aberration(2px); }
.fx-chroma-5 { filter: chromatic-aberration(5px); }
.fx-chroma-8 { filter: chromatic-aberration(8px); }
.fx-crt-light { filter: crt(0.3); }
.fx-crt-medium { filter: crt(0.5); }
.fx-crt-heavy { filter: crt(0.8); }
.fx-wave-subtle { filter: wave(2px, 0.3); }
.fx-wave-medium { filter: wave(4px, 0.5); }
.fx-wave-heavy { filter: wave(8px, 0.8); }
.fx-edge-soft { filter: edge-detect(0.2); }
.fx-edge-medium { filter: edge-detect(0.5); }
.fx-edge-hard { filter: edge-detect(0.8); }

/* Noise & Vignette variations */
.fx-noise-light { noise: 0.15; }
.fx-noise-medium { noise: 0.35; }
.fx-noise-heavy { noise: 0.6; }
.fx-vignette-light { vignette: 0.3; }
.fx-vignette-medium { vignette: 0.6; }
.fx-vignette-heavy { vignette: 0.9; }
.fx-tint-green { color-tint: rgba(0, 200, 100, 0.3); }
.fx-tint-purple { color-tint: rgba(139, 92, 246, 0.35); }
.fx-tint-amber { color-tint: rgba(245, 158, 11, 0.3); }

/* HSB-like adjustments */
.fx-saturate-high { filter: saturate(2.0); }
.fx-saturate-low { filter: saturate(0.3); }
.fx-hue-shift-90 { filter: hue-rotate(90deg); }
.fx-hue-shift-180 { filter: hue-rotate(180deg); }
.fx-hue-shift-270 { filter: hue-rotate(270deg); }
.fx-brightness-dark { filter: brightness(0.5); }
.fx-brightness-light { filter: brightness(1.5); }
.fx-brightness-high { filter: brightness(2.0); }

/* Complex chains */
.fx-chain-vintage { filter: sepia(40%) noise(0.15) vignette(0.4); }
.fx-chain-dreamy { filter: blur(1px) brightness(1.2) saturate(1.3); }
.fx-chain-dystopia { filter: grayscale(60%) contrast(1.4) noise(0.2); }
.fx-chain-neon { filter: brightness(1.3) saturate(1.8) chromatic-aberration(1px); }
.fx-chain-retro { filter: sepia(30%) crt(0.3) noise(0.1); }
.fx-chain-frost { filter: blur(1px) brightness(1.1) grayscale(20%); }

/* ═══ New Effects ═══ */

/* Glitch */
.fx-glitch-light  { filter: glitch(0.2); }
.fx-glitch-medium { filter: glitch(0.5); }
.fx-glitch-heavy  { filter: glitch(0.8); }

/* Swirl */
.fx-swirl-subtle { filter: swirl(0.8, 0.5); }
.fx-swirl-medium { filter: swirl(1.6, 0.5); }
.fx-swirl-heavy  { filter: swirl(3.14, 0.5); }

/* Bulge & Pinch */
.fx-bulge-light  { filter: bulge(0.3); }
.fx-bulge-medium { filter: bulge(0.6); }
.fx-bulge-heavy  { filter: bulge(1.0); }
.fx-pinch-light  { filter: pinch(0.3); }
.fx-pinch-medium { filter: pinch(0.6); }
.fx-pinch-heavy  { filter: pinch(1.0); }

/* Heat Haze */
.fx-heat-haze-subtle { filter: heat-haze(2px, 0.5); }
.fx-heat-haze-medium { filter: heat-haze(5px, 1.0); }
.fx-heat-haze-heavy  { filter: heat-haze(10px, 2.0); }

/* Refraction */
.fx-refract-subtle { filter: refraction(0.2, 1.2); }
.fx-refract-medium { filter: refraction(0.5, 1.5); }
.fx-refract-heavy  { filter: refraction(0.8, 2.0); }

/* Directional Blur */
.fx-dir-blur-h { filter: directional-blur(0deg, 2px); }
.fx-dir-blur-d { filter: directional-blur(45deg, 3px); }
.fx-dir-blur-v { filter: directional-blur(90deg, 2px); }

/* Motion Blur */
.fx-motion-blur-light  { filter: motion-blur(0deg, 2px); }
.fx-motion-blur-medium { filter: motion-blur(0deg, 4px); }
.fx-motion-blur-heavy  { filter: motion-blur(45deg, 6px); }

/* Radial / Zoom Blur */
.fx-radial-blur-light  { filter: radial-blur(0.1); }
.fx-radial-blur-medium { filter: radial-blur(0.25); }
.fx-radial-blur-heavy  { filter: radial-blur(0.4); }

/* Gradient Map */
.fx-gradient-map-bw   { filter: gradient-map(#000000, #ffffff); }
.fx-gradient-map-warm { filter: gradient-map(#1a0a00, #ffcc66); }
.fx-gradient-map-cool { filter: gradient-map(#001a33, #66ccff); }

/* Duotone */
.fx-duotone-cyan-pink   { filter: duotone(#00b4d8, #ec4899); }
.fx-duotone-purple-gold { filter: duotone(#7c3aed, #fbbf24); }
.fx-duotone-green-blue  { filter: duotone(#059669, #3b82f6); }

/* Color Grading */
.fx-grade-warm     { filter: color-grade(0.05, 1.1, 0.95); }
.fx-grade-cool     { filter: color-grade(-0.02, 0.95, 1.1); }
.fx-grade-cinematic { filter: color-grade(0.0, 1.2, 1.15); }

/* Silhouette */
.fx-silhouette-black  { filter: silhouette(#000000); }
.fx-silhouette-indigo { filter: silhouette(#6366f1); }
.fx-silhouette-white  { filter: silhouette(#ffffff); }

/* Hologram */
.fx-hologram-cyan        { filter: hologram(#22d3ee, 0.4); }
.fx-hologram-cyan-strong { filter: hologram(#22d3ee, 0.8); }
.fx-hologram-pink        { filter: hologram(#ec4899, 0.5); }

/* Lens Flare */
.fx-lens-flare-low    { filter: lens-flare(0.7); }
.fx-lens-flare-medium { filter: lens-flare(0.5); }
.fx-lens-flare-high   { filter: lens-flare(0.3); }

/* Dissolve */
.fx-dissolve-25 { filter: dissolve(0.25); }
.fx-dissolve-50 { filter: dissolve(0.5); }
.fx-dissolve-75 { filter: dissolve(0.75); }

/* Mask Reveal */
.fx-mask-reveal-25 { filter: mask-reveal(0.25); }
.fx-mask-reveal-50 { filter: mask-reveal(0.5); }
.fx-mask-reveal-75 { filter: mask-reveal(0.75); }

/* Transitions for new effects */
.fx-trans-dissolve {
    transition: filter 400ms ease;
    &:hover { filter: dissolve(0.6); }
}
.fx-trans-glitch {
    transition: filter 400ms ease;
    &:hover { filter: glitch(0.6); }
}
.fx-trans-swirl {
    transition: filter 400ms ease;
    &:hover { filter: swirl(1.6, 0.5); }
}
.fx-trans-mask-reveal {
    transition: filter 600ms ease;
    &:hover { filter: mask-reveal(1.0); }
}

/* Advanced chain presets */
.fx-chain-cyberpunk   { filter: glitch(0.3) chromatic-aberration(2px) crt(0.3); }
.fx-chain-underwater  { filter: duotone(#001a33, #00b4d8) heat-haze(2px, 0.5); }
.fx-chain-hologram-mix { filter: hologram(#22d3ee, 0.5) noise(0.1); }
