"use client";

import { useState } from "react";
import { ChevronDown, ChevronRight, FileText, Folder } from "lucide-react";
import type { TreeNode } from "@/types/vault";
import { useEditorStore } from "@/lib/store/editorStore";

interface Props {
  tree: TreeNode;
}

export function FileExplorer({ tree }: Props) {
  return (
    <div className="text-[13px] select-none">
      {tree.children.map((child) => (
        <TreeRow key={child.path} node={child} depth={0} />
      ))}
    </div>
  );
}

function TreeRow({ node, depth }: { node: TreeNode; depth: number }) {
  const [open, setOpen] = useState(depth < 1);
  const activePath = useEditorStore((s) => s.activePath);
  const openFile = useEditorStore((s) => s.openFile);

  const indent = { paddingLeft: 8 + depth * 14 };
  const isActive = !node.is_dir && activePath === node.path;

  if (node.is_dir) {
    return (
      <div>
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className="flex items-center gap-1 w-full py-1 pr-2 text-left hover:bg-[var(--color-surface-hover)] text-[var(--color-text-dim)]"
          style={indent}
        >
          {open ? (
            <ChevronDown size={12} className="opacity-60" />
          ) : (
            <ChevronRight size={12} className="opacity-60" />
          )}
          <Folder size={13} className="opacity-70" />
          <span className="truncate">{node.name}</span>
        </button>
        {open &&
          node.children.map((child) => (
            <TreeRow key={child.path} node={child} depth={depth + 1} />
          ))}
      </div>
    );
  }

  return (
    <button
      type="button"
      onClick={() => openFile(node.path)}
      className={
        "flex items-center gap-2 w-full py-1 pr-2 text-left hover:bg-[var(--color-surface-hover)] " +
        (isActive
          ? "bg-[var(--color-surface-hover)] text-[var(--color-text)]"
          : "text-[var(--color-text-dim)]")
      }
      style={{ paddingLeft: 8 + depth * 14 + 12 }}
    >
      <FileText size={13} className="opacity-70 shrink-0" />
      <span className="truncate">{node.name.replace(/\.(md|markdown)$/, "")}</span>
    </button>
  );
}
