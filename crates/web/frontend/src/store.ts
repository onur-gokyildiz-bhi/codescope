import { createSignal } from 'solid-js';

// Graph data
export const [graphData, setGraphData] = createSignal<{ nodes: any[]; links: any[] }>({ nodes: [], links: [] });
export const [selectedNode, setSelectedNode] = createSignal<any>(null);
export const [graphDepth, setGraphDepth] = createSignal(2);
export const [centerNode, setCenterNode] = createSignal<string | null>(null);
export const [hoveredNode, setHoveredNode] = createSignal<any>(null);

// UI panels
export const [showFiles, setShowFiles] = createSignal(false);
export const [showSidebar, setShowSidebar] = createSignal(false);
export const [showConv, setShowConv] = createSignal(false);
export const [showPalette, setShowPalette] = createSignal(false);
export const [showShortcuts, setShowShortcuts] = createSignal(false);
export const [splitMode, setSplitMode] = createSignal(false);

// Views
export const [viewMode, setViewMode] = createSignal<'graph' | 'pack' | 'hotspot' | 'cluster' | 'archive' | 'dream'>('graph');
export const [colorMode, setColorMode] = createSignal<'type' | 'module'>('type');

// Source viewer
export const [currentFile, setCurrentFile] = createSignal<string | null>(null);
export const [fileContent, setFileContent] = createSignal<any>(null);

// Physics
export const [repelStrength, setRepelStrength] = createSignal(-60);
export const [linkDistance, setLinkDistance] = createSignal(50);
export const [centerGravity, setCenterGravity] = createSignal(0.1);

// Stats
export const [stats, setStats] = createSignal<any>(null);

// Filter
export const [kindFilter, setKindFilter] = createSignal<string | null>(null);

// Loading & errors
export const [loading, setLoading] = createSignal(false);
export const [errorMsg, setErrorMsg] = createSignal<string | null>(null);

// Project (multi-project daemon mode)
export const [currentProject, setCurrentProject] = createSignal<string | null>(null);
export const [availableProjects, setAvailableProjects] = createSignal<string[]>([]);
export const [projectVersion, setProjectVersion] = createSignal(0);
