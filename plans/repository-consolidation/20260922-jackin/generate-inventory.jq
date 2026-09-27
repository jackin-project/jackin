def rows($s):
  $s
  | split("\n")
  | map(select(length > 0)
      | (split("\t") as $p
         | {ref:$p[0], oid:$p[1], authored_at:$p[2], subject:($p[3] // "")}));

def worktree_rows($s):
  $s
  | split("\n\n")
  | map(select(length > 0)
      | (split("\n") as $l
         | {
             path: (($l[] | select(startswith("worktree ")) | sub("^worktree "; "")) // null),
             head: (($l[] | select(startswith("HEAD ")) | sub("^HEAD "; "")) // null),
             branch: (($l[] | select(startswith("branch ")) | sub("^branch "; "")) // null),
             prunable: any($l[]; startswith("prunable"))
           }));

def state_rows($s):
  $s
  | split("\n")
  | map(select(length > 0)
      | (split("\t") as $p
         | {
             path:$p[0], head:($p[1] // ""), branch:($p[2] // ""),
             exists:(($p[4] // "0") | tonumber), dirty:(($p[5] // "0") | tonumber),
             tracked_changes:(($p[6] // "0") | tonumber),
             untracked:(($p[7] // "0") | tonumber), ignored:(($p[8] // "0") | tonumber),
             operations:($p[9] // "")
           }));

def pull_summary($p):
  $p
  | map({
      number, state, title, url,
      head:{ref:.head.ref, sha:.head.sha},
      base:{ref:.base.ref, sha:.base.sha},
      merged_at, closed_at, created_at, updated_at, draft,
      author:(.user.login // null),
      labels:([.labels[]?.name] | sort)
    });

rows($refs) as $r
| worktree_rows($worktrees) as $w
| state_rows($worktree_state) as $ws
| pull_summary(($pulls | add)) as $p
| {
    audit_id:$audit_id,
    started_at:$started_at,
    canonical_checkout:$canonical,
    starting_head:$starting_head,
    upstream_main:{ref:"origin/main", oid:$upstream_main},
    recovery_root:$recovery,
    scan:{
      host:"Darwin arm64 host; filesystem visibility limited to this user/runtime",
      roots:["/Users/donbeave", "/private/tmp", "/Users/donbeave/Projects/jackin-project"],
      exclusions:["unrelated user-file contents", "unmounted/inaccessible volumes"],
      limitations:["remote third-party repositories are not deletion targets", "container volumes require attributable evidence"]
    },
    counts:{
      all_refs:($r|length),
      local_heads:([$r[]|select(.ref|startswith("refs/heads/"))]|length),
      origin_branches:([$r[]|select((.ref|startswith("refs/remotes/origin/")) and ((.ref|test("^refs/remotes/origin/pr/"))|not))]|length),
      origin_pr_heads:([$r[]|select(.ref|startswith("refs/remotes/origin/pr/"))]|length),
      tags:([$r[]|select(.ref|startswith("refs/tags/"))]|length),
      worktrees:($w|length),
      worktree_state_records:($ws|length),
      existing_worktrees:([$ws[]|select(.exists==1)]|length),
      dirty_worktrees:([$ws[]|select(.dirty==1)]|length),
      github_prs:($p|length),
      open_prs:([$p[]|select(.state=="open")]|length),
      closed_unmerged_prs:([$p[]|select(.state=="closed" and .merged_at==null)]|length),
      merged_prs:([$p[]|select(.merged_at!=null)]|length)
    },
    refs:{
      local_heads:[$r[]|select(.ref|startswith("refs/heads/"))],
      origin_branches:[$r[]|select((.ref|startswith("refs/remotes/origin/")) and ((.ref|test("^refs/remotes/origin/pr/"))|not))],
      origin_pr_heads:[$r[]|select(.ref|startswith("refs/remotes/origin/pr/"))],
      tags:[$r[]|select(.ref|startswith("refs/tags/"))]
    },
    worktrees:$w,
    worktree_states:$ws,
    github_prs:$p
  }
