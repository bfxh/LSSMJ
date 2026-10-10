#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LSSMJ 证据恢复工具（F02 / 阶段 2 第一片）：按 targets-lock 复算或恢复证据源。

targets-lock.json 每条 = {target, url, commit, local_path, fetched_at, method}：
- method 含 "snapshot"：非 git 快照拷贝，无法 git 复算（check 单独计数，restore 拒绝）；
- 其余：git 克隆，钉住 commit——复算口径 = HEAD 与 commit 一致。

命令：
  python tools/restore.py check [--target X] [--json]
      盘点：OK（HEAD==commit）/ DRIFT（HEAD≠钉住提交，restore 可修）/
            MISSING（未克隆）/ BROKEN（目录在但非独立 git 克隆）/ SNAPSHOT（跳过计数）。
      非 snapshot 中存在 MISSING/DRIFT/BROKEN ⇒ exit 1（fail-closed，可当门用）。
  python tools/restore.py restore --target X [--force]
      把该目录恢复到钉住提交：init + fetch --depth 1 <commit> + checkout FETCH_HEAD。
      已有目录：非 git 克隆或工作区不干净（status --porcelain 非空）⇒ 拒绝（不毁现场）；
      --force 仅放宽「已存在同源克隆」的情形，脏工作区仍然拒绝。
      恢复后复验 HEAD == commit，不符即非零退出。

路径安全：local_path 必须（解析 symlink 后）落在仓根内且无 `..` 段——写入只经 git 子进程，
本工具自身不做任何可变路径写盘。
"""
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

LOCK = "docs/analysis/targets-lock.json"


def repo_root():
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def safe_local_path(p, root):
    """local_path 防线：无 `..` 段；解析 symlink 后必须在仓根内。"""
    if p and ".." in Path(p).parts:
        raise SystemExit(f"拒绝含 .. 的 local_path: {p}")
    ap = Path(p if os.path.isabs(p) else os.path.join(root, p))
    rp = Path(os.path.realpath(ap))
    rootp = Path(os.path.realpath(root))
    if rootp != rp and rootp not in rp.parents:
        raise SystemExit(f"local_path 解析后逃出仓根: {ap} → {rp}")
    return ap


def git(path, *args, timeout=600):
    try:
        r = subprocess.run(["git", "-C", str(path), *args],
                           capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, str(e)
    if r.returncode != 0:
        return None, (r.stderr or r.stdout).strip()
    return r.stdout.strip(), None


def is_real_clone(path: Path):
    """独立 git 克隆 = .git 存在且 toplevel 就是该目录（无 .git 的子目录会向上溯到主仓）。"""
    if not (path / ".git").exists():
        return False
    top, _ = git(path, "rev-parse", "--show-toplevel", timeout=30)
    if not top:
        return False
    try:
        return os.path.samefile(top, path)
    except OSError:
        return False


def load_lock(root):
    p = Path(root) / LOCK
    data = json.loads(p.read_text(encoding="utf-8"))
    items = data["targets"] if isinstance(data, dict) else data
    return items


def classify(entry, root):
    """返回 (state, detail, external)。state ∈ OK/DRIFT/MISSING/BROKEN/SNAPSHOT。

    external = 解析后落在仓根之外（#33 登记的外部证据根）——只读盘点，不入门、restore 拒写。
    """
    lp = safe_local_path_readonly(entry["local_path"])
    external = not _inside(lp, root)
    # 判别式：method 标明非 git 来源（快照/tar.gz/zip/gh api 单文件抓取）⇒ 无法 git 复算
    m = entry.get("method", "").lower()
    if any(k in m for k in ("snapshot", "快照", "tar.gz", "zip", "gh api")):
        return "SNAPSHOT", "非 git 来源（复算前需归档/解压，阶段 2 后续）", external
    if not lp.exists():
        return "MISSING", "目录不存在（restore 可恢复）", external
    if not is_real_clone(lp):
        return "BROKEN", "目录存在但不是独立 git 克隆", external
    head, err = git(lp, "rev-parse", "HEAD", timeout=30)
    if head is None:
        return "BROKEN", f"git rev-parse 失败: {err}", external
    if head == entry["commit"]:
        return "OK", head[:12], external
    return "DRIFT", f"HEAD {head[:12]} ≠ 钉住 {entry['commit'][:12]}（restore 可修）", external


def _inside(p, root):
    try:
        rp = Path(os.path.realpath(p))
        rootp = Path(os.path.realpath(root))
    except OSError:
        return False
    return rootp == rp or rootp in rp.parents


def safe_local_path_readonly(p):
    """只读盘点用：拒绝 `..` 段，不限制仓根（外部证据根允许盘点，写入仍在 restore 里拦）。"""
    if p and ".." in Path(p).parts:
        raise SystemExit(f"拒绝含 .. 的 local_path: {p}")
    return Path(p if os.path.isabs(p) else p)


def cmd_check(args):
    root = repo_root()
    items = load_lock(root)
    if args.target:
        items = [i for i in items if i["target"] == args.target]
        if not items:
            print(f"FAIL: lock 中无 target {args.target!r}")
            return 1
    counts = {}
    rows = []
    for e in items:
        state, detail, external = classify(e, root)
        key = f"{state}{'/external' if external else ''}"
        counts[key] = counts.get(key, 0) + 1
        rows.append((state, external, e, detail))
    if args.json:
        print(json.dumps({
            "counts": counts,
            "rows": [{"state": s, "external": x, "target": e["target"],
                      "commit": e["commit"], "local_path": e["local_path"],
                      "detail": d}
                     for s, x, e, d in rows],
        }, ensure_ascii=False, indent=1))
    else:
        for s, x, e, d in rows:
            mark = {"OK": "ok    ", "DRIFT": "DRIFT ", "MISSING": "MISS  ",
                    "BROKEN": "BROKEN", "SNAPSHOT": "snap  "}[s]
            tag = " [external]" if x else ""
            print(f"{mark} {e['target']:<28} {d}{tag}")
        print("counts:", counts)
    # 门只对仓内条目 fail-closed（外部证据根是登记过的作者机依赖，阶段 2 处理）
    bad_internal = sum(1 for s, x, _, _ in rows
                       if not x and s in ("MISSING", "DRIFT", "BROKEN"))
    if bad_internal:
        print(f"FAIL: {bad_internal} 个仓内证据源不可复算（restore 可修；snapshot "
              f"{counts.get('SNAPSHOT', 0)}、external {sum(1 for _, x, _, _ in rows if x)} 另计）")
        return 1
    print(f"OK: 仓内 {counts.get('OK', 0)} 个 git 证据源与钉住提交一致"
          f"（snapshot {counts.get('SNAPSHOT', 0)} / external "
          f"{sum(1 for _, x, _, _ in rows if x)} 另计）")
    return 0


def cmd_restore(args):
    root = repo_root()
    items = load_lock(root)
    matches = [e for e in items if e["target"] == args.target]
    if not matches:
        print(f"FAIL: lock 中无 target {args.target!r}")
        return 1
    e = matches[0]
    if "snapshot" in e.get("method", ""):
        print(f"FAIL: {e['target']} 是快照拷贝（method=snapshot），git 无法恢复——需先归档（阶段 2）")
        return 1
    lp = safe_local_path_readonly(e["local_path"])
    if not _inside(lp, root):
        print(f"FAIL: {e['target']} 的 local_path 在仓根之外（{lp}）——外部证据根不归本工具写，"
              f"阶段 2 路径迁移处理")
        return 1
    lp = safe_local_path(e["local_path"], root)
    url, commit = e["url"], e["commit"]
    if url == "unknown（本地克隆无 origin remote）" or not url.startswith(("http", "git@", "ssh")):
        print(f"FAIL: {e['target']} 的 url 不可恢复: {url!r}")
        return 1
    if lp.exists():
        if not is_real_clone(lp):
            print(f"FAIL: {lp} 存在但不是独立 git 克隆——请人工处理（不毁现场）")
            return 1
        dirty, _ = git(lp, "status", "--porcelain", timeout=120)
        if dirty:
            print(f"FAIL: {lp} 工作区不干净（{len(dirty.splitlines())} 处改动）——拒绝覆盖（--force 也不放行脏工作区）")
            return 1
        if not args.force:
            print(f"FAIL: {lp} 已存在（HEAD 可用 check 查看）。确要重置到钉住提交请加 --force")
            return 1
        head, err = git(lp, "rev-parse", "HEAD", timeout=30)
        if head == commit:
            print(f"ok: {e['target']} 已在钉住提交 {commit[:12]}")
            return 0
        # 同源克隆：fetch + 硬重置到钉住提交（干净工作区已确认）
        for cmd in (["fetch", "--depth", "1", "origin", commit],
                    ["checkout", "--detach", commit]):
            out, err = git(lp, *cmd)
            if out is None:
                print(f"FAIL: git {' '.join(cmd[:3])}…: {err}")
                return 1
    else:
        lp.parent.mkdir(parents=True, exist_ok=True)
        steps = [["init", str(lp)],
                 ["-C", str(lp), "remote", "add", "origin", url],
                 ["-C", str(lp), "fetch", "--depth", "1", "origin", commit],
                 ["-C", str(lp), "checkout", "--detach", "FETCH_HEAD"]]
        for cmd in steps:
            out, err = git(*cmd) if cmd[0] != "init" else git(root, *cmd)
            if out is None:
                print(f"FAIL: git {' '.join(cmd)}: {err}")
                return 1
    head, err = git(lp, "rev-parse", "HEAD", timeout=30)
    if head != commit:
        print(f"FAIL: 恢复后 HEAD {str(head)[:12]} ≠ 钉住 {commit[:12]}")
        return 1
    print(f"ok: {e['target']} 已恢复到钉住提交 {commit[:12]}")
    return 0


def main():
    ap = argparse.ArgumentParser(description="LSSMJ 证据源恢复（F02）")
    sub = ap.add_subparsers(dest="cmd", required=True)

    pc = sub.add_parser("check")
    pc.add_argument("--target", default=None)
    pc.add_argument("--json", action="store_true")

    pr = sub.add_parser("restore")
    pr.add_argument("--target", required=True)
    pr.add_argument("--force", action="store_true")

    args = ap.parse_args()
    if args.cmd == "check":
        return cmd_check(args)
    if args.cmd == "restore":
        return cmd_restore(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
