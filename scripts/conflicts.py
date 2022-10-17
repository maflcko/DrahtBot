from github import Github
import sys
import shutil
import time
import subprocess
import argparse
import os
import tempfile

from util.util import (
    return_with_pull_metadata,
    call_git,
    get_git,
    IdComment,
    update_metadata_comment,
    get_section_text,
)

MERGE_STRATEGY = "ort"  # https://github.blog/changelog/2022-09-12-merge-commits-now-created-using-the-merge-ort-strategy/
UPSTREAM_PULL = "upstream-pull"
ID_CONFLICTS_SEC = IdComment.SEC_CONFLICTS.value


def calc_merged(pulls_mergeable, base_branch):
    base_id = get_git(["log", "-1", "--format=%H", f"origin/{base_branch}"])
    for p in pulls_mergeable:
        call_git(["checkout", base_id, "--quiet"])
        call_git(  # May fail intermittently, if the GitHub metadata is temporarily inconsistent
            [
                "merge",
                f"--strategy={MERGE_STRATEGY}",
                "--quiet",
                f"{p.CON_commit}",
                "-m",
                f"Prepare base for {p.CON_id}",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        p.CON_merge_id = get_git(["log", "-1", "--format=%H", "HEAD"])


def calc_conflicts(pulls_mergeable, pull_check):
    conflicts = []
    base_id = get_git(["log", "-1", "--format=%H", pull_check.CON_merge_id])
    for i, pull_other in enumerate(pulls_mergeable):
        if pull_check.CON_id == pull_other.CON_id:
            continue
        call_git(["checkout", base_id, "--quiet"])
        try:
            call_git(
                [
                    "merge",
                    f"--strategy={MERGE_STRATEGY}",
                    "--quiet",
                    f"{pull_other.CON_commit}",
                    "-m",
                    f"Merge base_{pull_check.CON_id}+{pull_other.CON_id}",
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except subprocess.CalledProcessError:
            call_git(["merge", "--abort"])
            conflicts += [pull_other]
    return conflicts


def update_comment(dry_run, pull, pulls_conflict):
    text = "\n### Conflicts\n"
    if not pulls_conflict:
        if not get_section_text(pull, ID_CONFLICTS_SEC):
            # No conflict and no section to update
            return
        # Update section for no conflicts
        text += "No conflicts as of last run."
        update_metadata_comment(pull, ID_CONFLICTS_SEC, text=text, dry_run=dry_run)
        return

    text += "Reviewers, this pull request conflicts with the following ones:\n"
    text += "".join(
        [
            f"\n* [#{p.CON_id.removeprefix(p.CON_slug)}]({p.html_url}) ({p.title.strip()} by {p.user.login})"
            for p in pulls_conflict
        ]
    )
    text += "\n\n"
    text += "If you consider this pull request important, please also help to review the conflicting pull requests. "
    text += "Ideally, start with the one that should be merged first."

    update_metadata_comment(pull, ID_CONFLICTS_SEC, text=text, dry_run=dry_run)


def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(
        description="Determine conflicting pull requests.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--pull_id",
        help="Update the conflict comment and label for this pull request. Format: slug/number.",
        default="",
    )
    parser.add_argument(
        "--update_comments",
        action="store_true",
        help="Update all conflicts comments and labels.",
        default=False,
    )
    parser.add_argument(
        "--scratch_dir",
        help="The local dir used for scratching",
        default=os.path.join(THIS_FILE_PATH, "..", "scratch", "conflicts"),
    )
    parser.add_argument(
        "--github_access_token", help="The access token for GitHub.", default=""
    )
    parser.add_argument(
        "--github_repos",
        help="The comma-separated repo slugs of the monotree remotes on GitHub.",
        default="bitcoin-core/gui,bitcoin/bitcoin",
    )
    parser.add_argument(
        "--dry_run",
        help="Print changes/edits instead of calling the GitHub API.",
        action="store_true",
        default=False,
    )
    args = parser.parse_args()

    args.scratch_dir = os.path.join(args.scratch_dir, "")
    os.makedirs(args.scratch_dir, exist_ok=True)
    repo_dir = os.path.join(args.scratch_dir, args.github_repos.replace("/", "_"))
    if not os.path.isdir(repo_dir):
        for slug in args.github_repos.split(","):
            url = f"https://github.com/{slug}"
            print(f"Clone {url} repo to {repo_dir}")
            os.chdir(args.scratch_dir)
            if not os.path.isdir(repo_dir):
                call_git(["clone", "--quiet", url, repo_dir])
            print("Set git metadata")
            os.chdir(repo_dir)
            with open(os.path.join(repo_dir, ".git", "config"), "a") as f:
                f.write(f'[remote "{UPSTREAM_PULL}/{slug}"]\n')
                f.write(f"    url = {url}\n")
                f.write(f"    fetch = +refs/pull/*:refs/remotes/upstream-pull/*\n")
                f.flush()
            call_git(["config", "user.email", "no@ne.nl"])
            call_git(["config", "user.name", "none"])
            call_git(["config", "gc.auto", "0"])

    print(f"Fetching diffs for {args.github_repos} ...")
    os.chdir(repo_dir)
    call_git(["fetch", "--quiet", "--all"])

    github_api = Github(args.github_access_token)
    pull_blobs = []
    for slug in args.github_repos.split(","):
        print(f"Fetching open pulls for {slug} ...")
        github_repo = github_api.get_repo(slug)
        base_name = github_repo.default_branch
        pulls = return_with_pull_metadata(
            lambda: [p for p in github_repo.get_pulls(state="open", base=base_name)]
        )

        print(f"Open {base_name}-pulls for {slug}: {len(pulls)}")
        pulls_mergeable = [p for p in pulls if p.mergeable]
        print(f"Open mergeable {base_name}-pulls for {slug}: {len(pulls_mergeable)}")
        pull_blobs.append(pulls_mergeable)

    mono_pulls_mergeable = []
    for slug, ps in zip(args.github_repos.split(","), pull_blobs):
        print(f"Store diffs for {slug}")
        call_git(["fetch", "--quiet", f"{UPSTREAM_PULL}/{slug}"])
        for p in ps:
            p.CON_commit = get_git(
                ["log", "-1", "--format=%H", f"{UPSTREAM_PULL}/{p.number}/head"]
            )
            p.CON_slug = slug + "/"
            p.CON_id = f"{slug}/{p.number}"
    mono_pulls_mergeable.extend(ps)
    call_git(["fetch", "origin", base_name, "--quiet"])

    with tempfile.TemporaryDirectory() as temp_git_work_tree:
        shutil.copytree(
            os.path.join(repo_dir, ".git"),
            os.path.join(temp_git_work_tree, ".git"),
        )
        os.chdir(temp_git_work_tree)

        print("Calculate merged pulls")
        calc_merged(pulls_mergeable=mono_pulls_mergeable, base_branch=base_name)

        if args.update_comments:
            for i, pull_update in enumerate(mono_pulls_mergeable):
                print(
                    f"{i}/{len(mono_pulls_mergeable)} Checking for conflicts {base_name} <> {pull_update.CON_id} <> other_pulls ... "
                )
                pulls_conflict = calc_conflicts(
                    pulls_mergeable=mono_pulls_mergeable,
                    pull_check=pull_update,
                )
                update_comment(
                    dry_run=args.dry_run,
                    pull=pull_update,
                    pulls_conflict=pulls_conflict,
                )

        if args.pull_id:
            pull_merge = [p for p in mono_pulls_mergeable if p.CON_id == args.pull_id]

            if not pull_merge:
                print(
                    f"{args.pull_id} not found in all {len(mono_pulls_mergeable)} open, mergeable {base_name} pulls"
                )
                sys.exit(-1)
            pull_merge = pull_merge[0]

            print(
                f"Checking for conflicts {base_name} <> {pull_merge.CON_id} <> other_pulls ... "
            )
            conflicts = calc_conflicts(
                pulls_mergeable=pulls_mergeable,
                pull_check=pull_merge,
            )

            update_comment(
                dry_run=args.dry_run, pull=pull_merge, pulls_conflict=conflicts
            )

        os.chdir(repo_dir)


if __name__ == "__main__":
    main()
