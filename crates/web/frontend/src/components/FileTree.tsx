import { createSignal, createEffect, For, Show } from 'solid-js';
import { setCenterNode, setShowFiles, projectVersion, setErrorMsg } from '../store';
import { api } from '../api';

interface TreeNode {
  name: string;
  path: string;
  isDir: boolean;
  children: TreeNode[];
}

function buildTree(paths: string[]): TreeNode[] {
  const root: TreeNode = { name: '', path: '', isDir: true, children: [] };

  for (const p of paths) {
    const parts = p.split('/').filter(Boolean);
    let current = root;
    let accumulated = '';

    for (let i = 0; i < parts.length; i++) {
      accumulated += (accumulated ? '/' : '') + parts[i];
      const isLast = i === parts.length - 1;
      let child = current.children.find(c => c.name === parts[i]);
      if (!child) {
        child = { name: parts[i], path: accumulated, isDir: !isLast, children: [] };
        current.children.push(child);
      }
      current = child;
    }
  }

  // Sort: directories first, then alphabetical
  function sortTree(nodes: TreeNode[]) {
    nodes.sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    nodes.forEach(n => sortTree(n.children));
  }
  sortTree(root.children);
  return root.children;
}

function TreeItem(props: { node: TreeNode; depth: number }) {
  const [expanded, setExpanded] = createSignal(props.depth < 1);

  function onClick() {
    if (props.node.isDir) {
      setExpanded(v => !v);
    } else {
      setCenterNode(props.node.path);
    }
  }

  return (
    <>
      <div class={`file-tree-item ${props.node.isDir ? 'file-tree-dir' : ''}`} onClick={onClick}>
        {Array.from({ length: props.depth }).map(() => (
          <span class="file-tree-indent" />
        ))}
        <Show when={props.node.isDir}>
          <span class="file-tree-toggle">{expanded() ? '\u25BE' : '\u25B8'}</span>
        </Show>
        <Show when={!props.node.isDir}>
          <span class="file-tree-toggle">&middot;</span>
        </Show>
        <span>{props.node.name}</span>
      </div>
      <Show when={props.node.isDir && expanded()}>
        <For each={props.node.children}>
          {child => <TreeItem node={child} depth={props.depth + 1} />}
        </For>
      </Show>
    </>
  );
}

export default function FileTree() {
  const [tree, setTree] = createSignal<TreeNode[]>([]);

  createEffect(async () => {
    projectVersion(); // re-fetch on project switch
    try {
      const files = await api.files();
      const paths: string[] = Array.isArray(files)
        ? files.map((f: any) => typeof f === 'string' ? f : f.path || f.name || '')
        : [];
      setTree(buildTree(paths));
    } catch (e) { setErrorMsg(`Failed to load file tree: ${String(e)}`); }
  });

  return (
    <>
      <div class="panel-header">
        <span>Files</span>
        <button class="panel-close" onClick={() => setShowFiles(false)}>&times;</button>
      </div>
      <div class="panel-body" style="max-height:calc(100vh - 120px);overflow-y:auto">
        <For each={tree()}>
          {node => <TreeItem node={node} depth={0} />}
        </For>
        <Show when={tree().length === 0}>
          <span style="color:var(--text-dim);font-size:12px">No files indexed</span>
        </Show>
      </div>
    </>
  );
}
