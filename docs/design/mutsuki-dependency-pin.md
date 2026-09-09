# Mutsuki 依赖：PATH / GIT 切换

LiliaCode 通过根  的  统一 pin Host + AgentKit。
所有  条目必须使用同一模式与同一 revision，禁止混用 path / git。

远端仓库：（本地 checkout 目录名可能仍为 ）。

## 当前默认：GIT 模式

根  当前为 **GIT pin**：

- 
- （短写 ）



验证：



用途：可提交 / CI / Release；与远端 Mutsuki workspace-fs symlink jail fix 对齐（见 Mutsuki PR https://github.com/sena-nana/Mutsuki/pull/173，commit ）。

## PATH 模式（本地 sibling，联调临时）

前提：仓库布局为



需要联调尚未 push 的 Mutsuki 改动时：

1. 将根  中全部  从 usage: git [-v | --version] [-h | --help] [-C <path>] [-c <name>=<value>]
           [--exec-path[=<path>]] [--html-path] [--man-path] [--info-path]
           [-p | --paginate | -P | --no-pager] [--no-replace-objects] [--no-lazy-fetch]
           [--no-optional-locks] [--no-advice] [--bare] [--git-dir=<path>]
           [--work-tree=<path>] [--namespace=<name>] [--config-env=<name>=<envvar>]
           <command> [<args>]

These are common Git commands used in various situations:

start a working area (see also: git help tutorial)
   clone     Clone a repository into a new directory
   init      Create an empty Git repository or reinitialize an existing one

work on the current change (see also: git help everyday)
   add       Add file contents to the index
   mv        Move or rename a file, a directory, or a symlink
   restore   Restore working tree files
   rm        Remove files from the working tree and from the index

examine the history and state (see also: git help revisions)
   bisect    Use binary search to find the commit that introduced a bug
   diff      Show changes between commits, commit and working tree, etc
   grep      Print lines matching a pattern
   log       Show commit logs
   show      Show various types of objects
   status    Show the working tree status

grow, mark and tweak your common history
   branch    List, create, or delete branches
   commit    Record changes to the repository
   merge     Join two or more development histories together
   rebase    Reapply commits on top of another base tip
   reset     Reset current HEAD to the specified state
   switch    Switch branches
   tag       Create, list, delete or verify a tag object signed with GPG

collaborate (see also: git help workflows)
   fetch     Download objects and refs from another repository
   pull      Fetch from and integrate with another repository or a local branch
   push      Update remote refs along with associated objects

'git help -a' and 'git help -g' list available subcommands and some
concept guides. See 'git help <command>' or 'git help <concept>'
to read about a specific subcommand or concept.
See 'git help git' for an overview of the system. +  改为 。
2. 更新 。
3. 再跑上节验证命令。

示例：



联调结束后务必切回 GIT pin，勿将 PATH 模式提交为默认。

## 切回 / 更新 GIT pin 检查清单

- [ ] 全部  同一模式（全 path 或全同一 git rev）
- [ ] 目标 crate 已在远端  revision 的 workspace 成员中
- [ ]  与文档、 注释一致
- [ ]  / 相关  通过
- [ ] 未把本地 secret、临时 endpoint 写进提交

## Lockfile note (this bump)

 source URLs were best-effort updated to  via API. Because this pin jumps from  across newer  onto the jail-fix branch, **run a local  (or delete/re-resolve mutsuki entries) and refresh  before merge** so dependency graphs/metadata match the new revision under .
