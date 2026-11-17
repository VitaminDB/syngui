/* === @keyframes === */

@keyframes slide-right {
    from { translate-x: 0px; }
    to { translate-x: 300px; }
}

@keyframes scale-pulse {
    from { scale: 0.5; }
    to { scale: 1.5; }
}

@keyframes rotate-full {
    from { rotate: 0; }
    to { rotate: 360; }
}

@keyframes fade-in-out {
    from { opacity: 0.1; }
    to { opacity: 1.0; }
}

@keyframes breathing {
    from { scale: 0.8; }
    to { scale: 1.2; }
}

@keyframes slide-up-fade {
    from { translate-y: -20px; opacity: 0; }
    to { translate-y: 0px; opacity: 1; }
}

@keyframes combined {
    from { translate-x: 0px; scale: 0.5; rotate: 0; opacity: 0.3; }
    to { translate-x: 200px; scale: 1.2; rotate: 180; opacity: 1.0; }
}

@keyframes scale-x-stretch {
    from { scale-x: 1.0; }
    to { scale-x: 2.0; }
}

@keyframes pulse {
    from { opacity: 1; }
    50% { opacity: 0.3; }
    to { opacity: 1; }
}

@keyframes breathe {
    from { filter: blur(0px); }
    50% { filter: blur(4px); }
    to { filter: blur(0px); }
}

@keyframes hue-rotate {
    from { filter: hue-rotate(0deg); }
    50% { filter: hue-rotate(180deg); }
    to { filter: hue-rotate(360deg); }
}

@keyframes glow-pulse {
    from { glow: 0 0 12 rgba(99, 102, 241, 0.4); }
    50% { glow: 0 0 32 rgba(99, 102, 241, 0.9); }
    to { glow: 0 0 12 rgba(99, 102, 241, 0.4); }
}

@keyframes shadow-breathe {
    from { box-shadow: 0 2 8 rgba(0,0,0,0.1); }
    50% { box-shadow: 0 8 32 rgba(0,0,0,0.3); }
    to { box-shadow: 0 2 8 rgba(0,0,0,0.1); }
}

@keyframes color-shift {
    from { background: #3b82f6; }
    33% { background: #8b5cf6; }
    66% { background: #ec4899; }
    to { background: #3b82f6; }
}

@keyframes border-glow {
    from { border-color: rgba(99, 102, 241, 0.3); }
    50% { border-color: rgba(99, 102, 241, 1.0); }
    to { border-color: rgba(99, 102, 241, 0.3); }
}

@keyframes float {
    from { translate-y: 0px; }
    50% { translate-y: -10px; }
    to { translate-y: 0px; }
}

@keyframes shake {
    from { translate-x: 0px; }
    25% { translate-x: -5px; }
    50% { translate-x: 5px; }
    75% { translate-x: -3px; }
    to { translate-x: 0px; }
}

@keyframes spin-scale {
    from { rotate: 0; scale: 0.8; }
    50% { rotate: 180; scale: 1.2; }
    to { rotate: 360; scale: 0.8; }
}
