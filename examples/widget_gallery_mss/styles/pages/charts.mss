/* === Charts === */

LineChart {
    color: var(--text);
    background: var(--bg-surface);
    border-radius: 12;
    padding: 20;
    grid-color: var(--chart-grid);
    axis-color: var(--text-muted);
    axis-font-size: 11;
    title-font-size: 15;
    legend-font-size: 12;
    tooltip-background: var(--tooltip-bg);
    tooltip-border-color: var(--tooltip-border);
    animation-duration: 800;
    box-shadow: 0 1 3 var(--chart-shadow);
}

BarChart {
    color: var(--text);
    background: var(--bg-surface);
    border-radius: 12;
    padding: 20;
    grid-color: var(--chart-grid);
    axis-color: var(--text-muted);
    axis-font-size: 11;
    title-font-size: 15;
    animation-duration: 800;
    box-shadow: 0 1 3 var(--chart-shadow);
}

PieChart {
    color: var(--text);
    background: var(--bg-surface);
    border-radius: 12;
    padding: 16;
    label-color: var(--text-muted);
    label-font-size: 11;
    animation-duration: 800;
    box-shadow: 0 1 3 var(--chart-shadow);
}

RadarChart {
    color: var(--text);
    background: var(--bg-surface);
    border-radius: 12;
    padding: 16;
    grid-color: var(--chart-grid);
    label-color: var(--text-muted);
    label-font-size: 11;
    animation-duration: 800;
    box-shadow: 0 1 3 var(--chart-shadow);
}

GaugeChart {
    color: var(--text);
    background: var(--bg-surface);
    border-radius: 12;
    padding: 16;
    track-color: var(--chart-grid);
    needle-color: var(--text);
    label-color: var(--text-muted);
    label-font-size: 10;
    value-font-size: 28;
    animation-duration: 800;
    box-shadow: 0 1 3 var(--chart-shadow);
}

.charts-sidebar {
    width: 200px;
    background: var(--sidebar-bg);
    border-right: 1px solid var(--border);
}
