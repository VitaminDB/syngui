/* === 3D, частицы, «рыбий глаз» === */

.td-tile {
    width: 130px;
    height: 130px;
    border-radius: 18px;
    background: linear-gradient(135deg, #6a8dff, #b86bff);
    box-shadow: 0 10px 26px #00000044;
    transition: rotate-y 700ms ease-out-back, rotate-x 400ms ease-out-cubic, translate-z 300ms ease-out-back;
}
.td-tile-text { color: #ffffff; font-size: 14px; font-weight: bold; }
.td-flip:hover { rotate-y: 180deg; }
.td-tilt { perspective: 300px; }
.td-tilt:hover { rotate-x: 22deg; rotate-y: -24deg; }
.td-lift { background: linear-gradient(135deg, #ff8a5c, #ff5e8a); perspective: 400px; }
.td-lift:hover { translate-z: 60px; }
.td-coin { background: linear-gradient(135deg, #ffd24d, #ff9a1f); border-radius: 65px; animation: td-spin 2.4s linear infinite; }
@keyframes td-spin {
    from { rotate-y: 0deg; }
    to { rotate-y: 360deg; }
}
.td-reflect { background: linear-gradient(135deg, #2fd6a8, #2f8bd6); box-reflect: below 6px 0.4 60%; }
.td-anim { background: linear-gradient(135deg, #7dff9a, #2fbf71); }

.td-dock-area { height: 170px; padding-bottom: 10px; }
.td-dock {
    padding: 8px 10px;
    border-radius: 20px;
    background: linear-gradient(180deg, #ffffff30, #ffffff10);
    border-width: 1px;
    border-color: #ffffff40;
    background-rotate-x: 50deg;
    background-perspective: 420px;
    transform-origin: center bottom;
}
.td-dock-item {
    width: 56px;
    height: 56px;
    padding: 10px;
    border-radius: 14px;
    background: linear-gradient(135deg, #5c7cff, #9a5cff);
    box-reflect: below 4px 0.3 40%;
}
.td-dock-icon { icon-size: 36px; icon-color: #ffffff; }

.td-burst {
    padding: 10px 16px;
    border-radius: 12px;
    background-color: var(--surface-alt, #2b2d31);
}
.td-burst-text { font-size: 13px; }

.td-stream {
    width: 150px;
    height: 150px;
    border-radius: 14px;
    background-color: #14151c;
}
.td-fire { particle-preset: fire; particle-rate: 45; particle-emitter: point 50% 90%; particle-spread: 30deg; }
.td-snow { particle-preset: snow; particle-rate: 18; }
.td-smoke { particle-preset: smoke; particle-rate: 10; particle-emitter: point 50% 95%; }
.td-hover-fx { particle-preset: sparkle; particle-hover-rate: 40; particle-emitter: pointer; particle-lifetime: 0.4s 0.9s; }
.td-hover-box { background: linear-gradient(135deg, #2b2d55, #14151c); }
