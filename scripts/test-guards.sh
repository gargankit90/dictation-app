#!/usr/bin/env bash
# Tests for the guards: the git push guard (scripts/guard-push.sh, through
# scripts/pre-push) and the Claude Code hook (.claude/hooks/guard-git.sh).
# Part of the quality gate. Needs git and jq; touches nothing outside a temp dir.
set -euo pipefail

# Found from this file, not from git: the repo variables below are cleared.
root=$(cd "$(dirname "$0")/.." && pwd -P)

# git's pre-push hook exports GIT_DIR (and from a linked worktree it is an
# absolute path), and an inherited GIT_DIR beats `git -C <scratch>`: the
# scratch-repo commands below would then rewrite the real repo. Clear every
# repo-local variable git knows about, once, before any git command runs.
# shellcheck disable=SC2046 # word splitting is wanted: one name per word
unset $(git rev-parse --local-env-vars)

guard="$root/scripts/guard-push.sh"
pre_push="$root/scripts/pre-push"
hook="$root/.claude/hooks/guard-git.sh"
zero=0000000000000000000000000000000000000000

passed=0
failed=0

check() { # name expected-exit actual-exit
  if [ "$2" = "$3" ]; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
    echo "FAIL: $1 (expected exit $2, got $3)" >&2
  fi
}

# --- push guard -----------------------------------------------------------

tmp=$(cd "$(mktemp -d)" && pwd -P)
trap 'rm -rf "$tmp"' EXIT

g() { git -C "$tmp" -c user.name=test -c user.email=test@example.invalid "$@"; }
g init -q -b work
# Refuse to go on unless git really is working in the scratch repo.
if [ "$(g rev-parse --absolute-git-dir)" != "$tmp/.git" ]; then
  echo "test-guards: scratch repo resolves to $(g rev-parse --absolute-git-dir), not $tmp/.git; stopping before it touches a real repo" >&2
  exit 1
fi
g commit -q --allow-empty -m a
a=$(g rev-parse HEAD)
g commit -q --allow-empty -m b
b=$(g rev-parse HEAD)
g switch -q --orphan other
g commit -q --allow-empty -m c
c=$(g rev-parse HEAD)
unknown=1111111111111111111111111111111111111111

push() { # name expected-exit refs-line...
  local name=$1 expected=$2 code=0
  shift 2
  (cd "$tmp" && printf '%s\n' "$@" | "$guard" origin git@example.invalid:r.git) 2>/dev/null || code=$?
  check "guard-push: $name" "$expected" "$code"
}

push "new branch is allowed" 0 "refs/heads/work $b refs/heads/m1-x $zero"
push "fast-forward is allowed" 0 "refs/heads/work $b refs/heads/m1-x $a"
push "new tag is allowed" 0 "refs/tags/m1 $b refs/tags/m1 $zero"
push "push to main is refused" 1 "refs/heads/work $b refs/heads/main $a"
push "new main is refused" 1 "refs/heads/work $b refs/heads/main $zero"
push "deleting main is refused" 1 "(delete) $zero refs/heads/main $a"
push "deleting a branch is refused" 1 "(delete) $zero refs/heads/m1-x $a"
push "non-fast-forward is refused" 1 "refs/heads/other $c refs/heads/m1-x $b"
push "unknown remote commit is refused" 1 "refs/heads/work $b refs/heads/m1-x $unknown"
push "moving a tag is refused" 1 "refs/tags/m1 $b refs/tags/m1 $a"
push "one bad ref refuses the push" 1 \
  "refs/heads/work $b refs/heads/m1-x $zero" \
  "refs/heads/work $b refs/heads/main $a"

# scripts/pre-push must run the guard. Only refused pushes are tested: an
# allowed one would go on to run the full gate, which runs this script.
pre_push_refuses() { # name refs-line
  local code=0
  (cd "$root" && printf '%s\n' "$2" | "$pre_push" origin git@example.invalid:r.git) 2>/dev/null || code=$?
  check "pre-push: $1" 1 "$code"
}
pre_push_refuses "push to main is refused" "refs/heads/x $zero refs/heads/main $zero"
pre_push_refuses "deleting a branch is refused" "(delete) $zero refs/heads/m1-x $zero"

ROOT=$root
TMP_ROOT=$tmp
WT_BASE="$TMP_ROOT/Dictation App"
mkdir -p "$WT_BASE"
git init -q --bare "$WT_BASE/remote.git"
git clone -q "$WT_BASE/remote.git" "$WT_BASE/main-wt" 2>/dev/null
# Run in a separate `bash -euo pipefail` process (not a `( ... ) &&` subshell,
# which would disable `set -e` across intermediate checks). Also use explicit
# `die` helpers instead of bare `[[ a && b ]]`, because macOS `/bin/bash` 3.2
# suppresses `errexit` on `&&` inside `[[ ... ]]`.
if ROOT="$ROOT" WT_BASE="$WT_BASE" bash -euo pipefail <<'SEC3'; then
  die() { echo "sec3 fail: $*" >&2; exit 1; }
  assert_rc_contains() {
    local got_rc="$1" want_rc="$2" text="$3" needle="$4" label="$5"
    [ "$got_rc" -eq "$want_rc" ] || die "$label: want rc $want_rc, got $got_rc ($text)"
    case "$text" in
      *"$needle"*) ;;
      *) die "$label: missing '$needle' in: $text" ;;
    esac
  }
  assert_not_contains() {
    local text="$1" needle="$2" label="$3"
    case "$text" in
      *"$needle"*) die "$label: unexpected '$needle' in: $text" ;;
    esac
  }

  # Only this synthetic repository uses this runner. The application gate
  # and real hook files remain unchanged; CI need not install global lefthook.
  mkdir -p "$WT_BASE/bin"
  cat > "$WT_BASE/bin/lefthook" <<'RUNNER'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  install)
    hooks_dir="$(git rev-parse --git-common-dir)/hooks"
    mkdir -p "$hooks_dir"
    cat > "$hooks_dir/pre-commit" <<'HOOK'
#!/usr/bin/env bash
exec lefthook run pre-commit
HOOK
    chmod +x "$hooks_dir/pre-commit"
    ;;
  'run pre-commit') ;;
  'run pre-push-gate')
    echo SYNTHETIC_GATE_RAN >&2
    [ ! -e .fixture-gate-fails ]
    ;;
  *) echo "unexpected fixture lefthook arguments: $*" >&2; exit 1 ;;
esac
RUNNER
  chmod +x "$WT_BASE/bin/lefthook"
  export PATH="$WT_BASE/bin:$PATH"

  cd "$WT_BASE/main-wt"
  git config user.email "tester@example.com"
  git config user.name "Tester"
  git checkout -qb m0.1-sim
  echo "scaffold" > README.md
  git add README.md
  git commit -qm "m0.1"
  git push -q -u origin m0.1-sim
  git push -q origin HEAD:refs/heads/main

  git checkout -qb m0.2-sim
  mkdir -p scripts
  cp "$ROOT"/scripts/{guard-push.sh,pre-push,install-hooks.sh} scripts/
  # Synthetic repository: exercise the installed hooks without invoking the
  # application gate (which does not exist in this scratch checkout).
  cat > lefthook.yml <<'YAML'
pre-commit:
  jobs:
    - name: fixture
      run: "true"
pre-push-gate:
  jobs:
    - name: fixture
      run: "true"
YAML
  git add .
  git commit -qm "m0.2"
  ./scripts/install-hooks.sh >/dev/null

  thooks="$(git rev-parse --path-format=absolute --git-common-dir)/hooks"
  [ -x "$thooks/guard-push.sh" ]
  [ -x "$thooks/pre-push" ]
  cmp -s "$thooks/guard-push.sh" scripts/guard-push.sh

  # Linked worktree WITHOUT scripts/ (simulating pre-M0.2 branch)
  git worktree add -q --detach "$WT_BASE/m01-wt" origin/m0.1-sim
  cd "$WT_BASE/m01-wt"
  echo "report" > report.md
  git add report.md
  git commit -qm "test: report" 2>/dev/null

  git push --dry-run origin HEAD:refs/heads/m0.1-sim >/dev/null 2>&1
  git push origin HEAD:refs/heads/m0.1-sim >/dev/null 2>&1
  git push origin HEAD:refs/heads/m0.1-sim >/dev/null 2>&1 # no-op push
  git push --dry-run origin HEAD:refs/heads/m1-new >/dev/null 2>&1

  rc=0; out=$(git push --dry-run origin HEAD:main 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out" "guard-push: refused: pushing to main" "m01-wt push to main"

  git commit -q --amend -m "amended" 2>/dev/null
  rc=0; out=$(git push --dry-run --force origin HEAD:refs/heads/m0.1-sim 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out" "guard-push: refused: force push" "m01-wt force push"

  rc=0; out=$(git push --dry-run origin --delete m0.1-sim 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out" "guard-push: refused: deleting" "m01-wt branch delete"

  git tag t1 HEAD~1
  git push -q origin t1
  git tag -f t1 HEAD >/dev/null
  rc=0; out=$(git push --dry-run --force origin t1 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out" "guard-push: refused: moving existing tag" "m01-wt tag move"

  # Fail-closed when installed guard copy is missing
  mv "$thooks/guard-push.sh" "$WT_BASE/gp.bak"
  rc=0; out=$(git push --dry-run origin HEAD:refs/heads/m1-new 2>&1) || rc=$?
  mv "$WT_BASE/gp.bak" "$thooks/guard-push.sh"
  assert_rc_contains "$rc" 1 "$out" "missing; push refused" "missing guard-push.sh fail-closed"

  # Linked worktree WITH scripts/pre-push: verify chaining, diff note, no-op skip, and early refuse
  cd "$WT_BASE/main-wt"
  git worktree add -q -b m0.2-linked "$WT_BASE/m02-linked" HEAD
  cd "$WT_BASE/m02-linked"
  printf '\necho "CHECKOUT_PRE_PUSH_RAN" >&2\n' >> scripts/pre-push
  printf '\n# local drift test\n' >> scripts/guard-push.sh
  git add scripts/pre-push scripts/guard-push.sh
  git commit -qm "instrument" 2>/dev/null

  out=$(git push origin m0.2-linked 2>&1)
  assert_rc_contains 0 0 "$out" "CHECKOUT_PRE_PUSH_RAN" "linked pre-push ran"
  assert_rc_contains 0 0 "$out" "SYNTHETIC_GATE_RAN" "linked gate ran"

  # The unchanged pre-push script must propagate a failing gate, too.
  touch .fixture-gate-fails
  rc=0; out_fail=$(git push --dry-run origin HEAD:refs/heads/gate-failure 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out_fail" "SYNTHETIC_GATE_RAN" "linked gate failure refuses push"
  rm .fixture-gate-fails
  assert_rc_contains 0 0 "$out" "pre-push: note:" "linked drift note"

  out_noop=$(git push origin m0.2-linked 2>&1)
  assert_not_contains "$out_noop" "CHECKOUT_PRE_PUSH_RAN" "linked no-op skip"
  assert_not_contains "$out_noop" "SYNTHETIC_GATE_RAN" "linked no-op skips gate"

  rc=0; out_refuse=$(git push --dry-run origin HEAD:main 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out_refuse" "guard-push: refused: pushing to main" "linked main refuse"
  assert_not_contains "$out_refuse" "CHECKOUT_PRE_PUSH_RAN" "linked main early stop"
  assert_not_contains "$out_refuse" "SYNTHETIC_GATE_RAN" "linked main refuses before gate"
SEC3
  check "installed hooks + linked worktree push matrix" 0 0
else
  check "installed hooks + linked worktree push matrix" 0 1
fi


# --- Claude Code hook -----------------------------------------------------

hook_case() { # expected-exit command
  local code=0
  jq -n --arg c "$2" '{tool_input: {command: $c}}' | "$hook" 2>/dev/null || code=$?
  check "guard-git: $2" "$1" "$code"
}
blocked() { hook_case 2 "$1"; }
allowed() { hook_case 0 "$1"; }

allowed "git push --force origin m1-x"
allowed "git push --force-with-lease origin m1-x"
allowed "git push -f origin m1-x"
allowed "git push origin +m1-x"
allowed "git push origin :m1-x"
allowed "git push --delete origin m1-x"
allowed "git push -d origin m1-x"
allowed "git push origin main"
allowed "git push origin HEAD:main"
allowed "git push --all origin"
allowed "git -C . push --force origin m1-x"
allowed "cd repo && git push -f"
blocked "git commit --no-verify -m x"
blocked "git commit -n -m x"
blocked "git commit -anm x"
blocked "git -c core.hooksPath=/dev/null commit -m x"
blocked "LEFTHOOK=0 git commit -m x"
blocked "lefthook uninstall"
blocked "rm .git/hooks/pre-push"
blocked "gh pr merge 1 --merge"
blocked "gh api -X PUT repos/o/r/pulls/1/merge"
blocked "gh api -X PATCH repos/o/r/git/refs/heads/main"
blocked "gh repo delete o/r --yes"
blocked "gh release delete v1 --yes"
blocked "gh repo edit --visibility public"
blocked "echo x > scripts/guard-push.sh"
blocked "sed -i '' s/main/x/ scripts/pre-push"
blocked "rm lefthook.yml"
blocked "cp /tmp/x .claude/settings.json"
blocked "git checkout -- .claude/hooks/guard-git.sh"
blocked "python3 -c 'open(\"scripts/install-hooks.sh\",\"w\")'"

# Added with the hardened hook (M0.2 guard patches): patches, aliases and
# hookspath variants are blocked; message text no longer trips the checks.
blocked "git apply fix.patch"
blocked "git am 0001-fix.patch"
blocked "patch -p1 < fix.patch"
allowed "git config alias.p 'push --force'"
blocked "gh alias set m 'pr merge'"
blocked "git -c core.hookspath=/dev/null commit -m x"
allowed "git apply --check fix.patch"
allowed "git commit -m 'docs: explain -n and --no-verify are banned'"

# Fails closed on input it can't read.
raw_input_blocked() { # name input
  local code=0
  printf '%s' "$2" | "$hook" 2>/dev/null || code=$?
  check "guard-git: $1" 2 "$code"
}
raw_input_blocked "empty input is blocked" ""
raw_input_blocked "non-JSON input is blocked" "git push --force"

allowed "git push -u origin m0.3-quality-gate"
allowed "git push origin m1-maintenance"
allowed "git commit -m 'feat(core): add step'"
allowed "git status"
allowed "cat lefthook.yml"
allowed "gh pr create --base main --head m1-x"
allowed "gh pr comment 3 --body 'Tester: PASS'"
allowed "cargo clippy --workspace --all-targets -- -D warnings"

# Review Major 3: case-insensitive core.hooksPath, LEFTHOOK=false/LEFTHOOK_*, lefthook-local, .GIT/HOOKS, git-path, git alias
blocked 'git config core.hookspath /tmp/empty'
blocked 'git config --local Core.HooksPath /dev/null'
blocked 'git -c core.hookspath=/tmp/empty commit -m "x"'
blocked 'git -c CORE.HOOKSPATH=/tmp/empty push origin m0.2-agent-docs'
blocked "printf '[core]\\n\\thooksPath = /dev/null\\n' >> .git/config"
blocked 'LEFTHOOK=false git commit -m x'
blocked "LEFTHOOK='0' git commit -m x"
blocked 'export LEFTHOOK=FALSE; git commit -m x'
blocked 'LEFTHOOK_EXCLUDE=gate git push origin m1'
blocked 'lefthook uninstall'
blocked 'echo "pre-push: {skip: true}" > lefthook-local.yml'
blocked 'cp x .lefthook-local.yml'
blocked 'rm .GIT/HOOKS/pre-push'
blocked 'rm "$(git rev-parse --git-path hooks)/pre-push"'

# Minor #16: < and > redirections right after message flag clear pending flag
blocked 'git commit -m > scripts/guard-push.sh'
blocked 'git commit -m>scripts/guard-push.sh'
blocked 'gh pr comment 2 --body > .claude/hooks/guard-git.sh'
allowed 'git commit -m "x > y"'
allowed 'git commit -m "notes on scripts/guard-push.sh > ok"'

# Minor #18: expanded guard self-tamper check (subshells, relative paths, .claude/hooks/*, patches, 2>&1 / >/dev/null)
blocked '(rm scripts/guard-push.sh)'
blocked '(cp /dev/null scripts/pre-push)'
blocked 'cp /dev/null .claude/hooks/other.sh'
blocked 'rm -rf .claude/hooks'
blocked 'rm -rf .claude'
blocked 'rm -rf scripts'
blocked 'cd scripts && cp /dev/null pre-push'
blocked 'cd .claude && cp x settings.local.json'
blocked 'patch -p1 < fix.patch'
blocked "bash -c 'patch -p1 < x.diff'"
blocked 'git apply guard-fixes-2.patch'
blocked 'git apply --check x.patch && git apply x.patch'
blocked 'git am 0001-x.patch'
blocked 'sed -E -i "" s/x/y/ scripts/guard-push.sh'
blocked 'unlink scripts/pre-push'
blocked 'echo x 2>&1 > scripts/guard-push.sh'
blocked 'echo x >/dev/null > scripts/install-hooks.sh'
allowed 'git apply --check guard-fixes-2.patch'
allowed 'patch --dry-run -p1 < x.patch'
allowed 'git stash apply'
allowed 'cat scripts/guard-push.sh 2>&1'
allowed 'scripts/install-hooks.sh 2>&1 | tail -3'
allowed 'lefthook run pre-push-gate > /tmp/gate.log'


# G1: endpoint/method combinations and false positives.
for flag in '-X PUT' '-XPUT' '--method PUT' '--method=PUT'; do
  blocked "gh api $flag repos/o/r/pulls/12/merge"
done
blocked "gh api repos/o/r/merges --method POST"
blocked "gh alias import aliases.yml"
for method in DELETE PATCH PUT; do
  for endpoint in repos/o/r repos/o/r/rulesets/1 repos/o/r/branches/main repos/o/r/tags/v1 repos/o/r/git/refs/tags/v1; do
    blocked "gh api -X $method $endpoint"
  done
done
for endpoint in repos/o/r/rules/branches/main repos/o/r/rulesets/1 repos/o/r/branches/main repos/o/r/git/refs; do
  allowed "gh api $endpoint"
  allowed "gh api -X GET $endpoint"
done
allowed 'git commit -m "--no-verify"'
allowed 'git config core.hooksPath'
allowed 'git add crates/core/src/alias.rs'
allowed 'git log --grep="patch 2"'
allowed 'gh pr view 1 --json mergeable'
allowed 'git commit -m "push to main; patch 2; hooks; merges; --no-verify"'
allowed 'cat docs/hooks.md'
allowed 'echo "push to main hooks merges patch 2" > /tmp/spec.md'
allowed 'gh api -X GET repos/o/r/pulls/1/merge'
allowed 'gh api -X GET repos/o/r/merges'
allowed 'gh api -X POST repos/o/r/issues'
blocked '/usr/bin/git commit -n -m msg'
blocked 'gh -R o/r pr merge 2'
blocked 'gh --repo=o/r pr merge 2'
blocked 'bash -c "gh pr merge 2"'
blocked 'git commit -m "$(gh pr merge 2)"'
raw_input_blocked "array root" '[1,2]'
raw_input_blocked "multiple JSON values" '{} {}'
raw_input_blocked "non-string command" '{"tool_input":{"command":5}}'
raw_input_blocked "non-object tool input" '{"tool_input":"x"}'
# Literal patches are inspected, not executed.
cat > "$tmp/safe.patch" <<'PATCH'
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old
+new
PATCH
sed 's@README.md@scripts/pre-push@g' "$tmp/safe.patch" > "$tmp/guard.patch"
allowed "git apply $tmp/safe.patch"
blocked "git apply $tmp/guard.patch"
blocked "patch -p1 < $tmp/guard.patch"
allowed 'cp scripts/guard-push.sh /tmp/guard-copy'
blocked 'tee scripts/pre-push < /tmp/x'
blocked 'ln -sf /tmp/x .claude/hooks/guard-git.sh'
blocked 'mv /tmp/x lefthook.yml'
blocked 'gh pr -R o/r merge 2'
blocked $'git status;\ngh pr merge 2'
allowed 'gh pr comment 2 --body "merge"'
allowed 'gh pr comment 2 --body "push to main; hooks; merges; patch 2"'
# Round 1 B1: API option values never replace the endpoint.
for option in '-H "Accept: application/vnd.github+json"' '--header="X-GitHub-Api-Version: 2022-11-28"' '--jq .sha' '-q .sha' '-t "{{.sha}}"' '--template={{.sha}}' '--hostname github.com' '-p foo' '--preview=foo' '--cache 1h' '-f note=value' '-F note=value' '--field note=value' '--raw-field=note=value'; do
  for request in '-X PUT repos/o/r/pulls/186/merge' '-X POST repos/o/r/merges' '-X DELETE repos/o/r/rulesets/1' '-X PUT repos/o/r/rulesets/1' '-X PATCH repos/o/r/rulesets/1' '-X PATCH repos/o/r' '-X DELETE repos/o/r/git/refs/heads/work'; do
    blocked "gh api $option $request"
  done
  allowed "gh api $option --method GET repos/o/r/rulesets/1"
done
blocked 'gh api --method PUT -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" /repos/o/r/pulls/186/merge -f merge_method=merge'
blocked 'gh api -X PUT "repos/o/r/pulls/186/merge?x=1#fragment"'
blocked 'gh api -X DELETE "repos/o/r/rulesets/1?x=1"'
blocked 'gh api -X PUT ignored repos/o/r/pulls/186/merge'
allowed 'gh api -f note=value --method=get -H "Accept: application/json" repos/o/r/rulesets/1'
allowed 'gh api --method GET -f query=mergePullRequest graphql'
# --input is a value option, not an endpoint, even for REST requests.
printf '{}' > "$tmp/request.json"
blocked "gh api --input $tmp/request.json -X PUT repos/o/r/pulls/186/merge"
allowed "gh api --input=$tmp/request.json -X GET repos/o/r/rulesets/1"

# Round 1 M1: unwrap commands without treating quoted message data as code.
for wrapper in 'timeout 300' 'timeout -s TERM -k 5 300' 'nohup' 'xargs' 'xargs -n 1 -I {}' 'env' 'env -u DEBUG' 'command' 'exec' 'nice' 'nice -n 5' 'time' 'time -p' 'sudo' 'sudo -u root' 'caffeinate -t 300' 'stdbuf -oL'; do
  blocked "$wrapper gh pr merge 186 --merge"
  blocked "$wrapper gh api -H 'Accept: application/json' -X PUT repos/o/r/pulls/186/merge"
  allowed "$wrapper gh pr view 186 --json mergeable"
done
for shell in 'sh -c' 'bash -c' 'bash -lc' 'bash -ec' 'bash -xc' 'zsh -c' 'zsh -lc' 'eval'; do
  blocked "$shell 'gh pr merge 186 --merge'"
  blocked "$shell 'gh api -X POST repos/o/r/merges'"
  allowed "$shell 'gh pr view 186 --json mergeable'"
done
blocked 'echo 186 | xargs gh pr merge --merge'
blocked 'echo "$(timeout 300 gh api -X PUT repos/o/r/pulls/1/merge)"'
blocked 'echo `nohup gh pr merge 186`'
allowed 'git commit -m "timeout 300 gh pr merge; core.hooksPath; mergePullRequest; hooks; merges"'
allowed 'echo "gh api --method PUT -H headers repos/o/r/pulls/1/merge"'

# Round 1 M2: only GraphQL mutation payloads trigger the role/settings block.
for mutation in mergePullRequest enablePullRequestAutoMerge enqueuePullRequest mergeBranch createCommitOnBranch createRef deleteRef updateRef updateRefs updateRepository deleteBranchProtectionRule createBranchProtectionRule deleteRepositoryRuleset updateRepositoryRuleset createRepositoryRuleset; do
  blocked "gh api graphql -f 'query=mutation { $mutation(input:{}) { clientMutationId } }'"
  printf '{"query":"mutation { %s(input:{}) { clientMutationId } }"}' "$mutation" > "$tmp/mutation.json"
  blocked "gh api --input $tmp/mutation.json graphql"
done
printf '%s\n' '{"query":"query { repository(owner: \"o\", name: \"r\") { name } }"}' > "$tmp/query.json"
printf 'mutation { deleteRef(input:{}) { clientMutationId } }' > "$tmp/query.graphql"
blocked "gh api graphql -F query=@$tmp/query.graphql"
allowed "gh api --input=$tmp/query.json graphql"
allowed 'gh api graphql -f "query=query { repository { name } }"'
allowed 'gh api graphql -f '\''query=query { search(query:"mergePullRequest mutation", type:ISSUE) { issueCount } }'\'''
allowed 'gh pr comment 1 --body "mutation { mergePullRequest(input:{}) }"'

# Round 1 M3: environment and all git config scopes still enforce hook policy.
blocked 'GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git push --force origin work'
blocked 'env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_1=CORE.HOOKSPATH GIT_CONFIG_VALUE_1=/dev/null git push origin work'
blocked 'export GIT_CONFIG_KEY_0=core.hooksPath'
blocked 'GIT_CONFIG_PARAMETERS="'\''core.hooksPath'\''='\''/dev/null'\''" git push origin +work'
blocked 'export GIT_CONFIG_PARAMETERS="'\''core.hooksPath'\''='\''/dev/null'\''"'
blocked 'git --config-env=core.hooksPath=HP push --force origin work'
blocked 'git --config-env core.hooksPath=HP push origin work'
blocked 'git push -c core.hooksPath=/dev/null origin work'
blocked 'git -c core.hooksPath=/dev/null config --get user.name'
for scope in '--global' '--system' '--worktree' '--file /tmp/git-config'; do
  blocked "git config $scope core.hooksPath /dev/null"
  allowed "git config $scope --get core.hooksPath"
done
allowed 'GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=Tester git status'
allowed 'git --config-env=user.name=NAME status'
allowed 'git commit -m "GIT_CONFIG_KEY_0=core.hooksPath; --config-env=core.hooksPath=HP"'

# Block messages must name the actual role policy.
message=$(jq -n '{tool_input:{command:"gh pr merge 1"}}' | "$hook" 2>&1) || :
if [[ "$message" == *"Code Reviewer merges"* && "$message" == *"owner changes guards"* ]]; then code=0; else code=1; fi
check "guard-git: policy message" 0 "$code"
# Confirm Edit/Write protections remain in the unchanged settings deny list.
for path in .claude/settings.json '.claude/hooks/**' scripts/guard-push.sh scripts/pre-push scripts/install-hooks.sh lefthook.yml; do
  for tool in Edit Write; do
    code=0
    jq -e --arg rule "$tool(**/$path)" '.permissions.deny | index($rule) != null' "$root/.claude/settings.json" >/dev/null || code=$?
    check "settings: $tool $path" 0 "$code"
  done
done

# --- isolation from an inherited GIT_DIR ----------------------------------

# Run this script again the way git's pre-push hook does from a linked
# worktree: GIT_DIR, GIT_WORK_TREE and GIT_INDEX_FILE exported, here pointing
# at a throwaway repo. The run must pass and leave that repo exactly as it was.
if [ -z "${TEST_GUARDS_NESTED:-}" ]; then
  bait="$tmp/inherited"
  git init -q -b bait "$bait"
  git -C "$bait" -c user.name=test -c user.email=test@example.invalid commit -q --allow-empty -m bait
  snapshot() {
    git -C "$bait" symbolic-ref HEAD
    git -C "$bait" rev-parse HEAD
    git -C "$bait" for-each-ref --format='%(refname) %(objectname)'
    git -C "$bait" config --local --list
    git -C "$bait" count-objects
  }
  before=$(snapshot)
  code=0
  TEST_GUARDS_NESTED=1 GIT_DIR="$bait/.git" GIT_WORK_TREE="$bait" GIT_INDEX_FILE="$bait/.git/index" \
    "$root/scripts/test-guards.sh" >/dev/null 2>&1 || code=$?
  check "isolation: run with an inherited GIT_DIR passes" 0 "$code"
  if [ "$(snapshot)" = "$before" ]; then same=0; else same=1; fi
  check "isolation: the inherited GIT_DIR's repo is untouched" 0 "$same"
fi

echo "test-guards: $passed passed, $failed failed"
[ "$failed" -eq 0 ]
