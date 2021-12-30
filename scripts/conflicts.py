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

UPSTREAM_PULL = "upstream-pull"
ID_CONFLICTS_SEC = IdComment.SEC_CONFLICTS.value


def calc_conflicts(pulls_mergeable, num, base_branch):
    conflicts = []
    base_id = get_git(["log", "-1", "--format=%H", f"origin/{base_branch}"])
    call_git(["checkout", base_id, "--quiet"])
    call_git(
        [
            "merge",
            "--quiet",
            f"{UPSTREAM_PULL}/{num}/head",
            "-m",
            f"Prepare base for {num}",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    base_id = get_git(["log", "-1", "--format=%H", "HEAD"])
    for i, pull_other in enumerate(pulls_mergeable):
        if num == pull_other.number:
            continue
        call_git(["checkout", base_id, "--quiet"])
        try:
            call_git(
                [
                    "merge",
                    f"{UPSTREAM_PULL}/{pull_other.number}/head",
                    "-m",
                    f"Merge base_{num}+{pull_other.number}",
                    "--quiet",
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
            f"\n* [#{p.number}]({p.html_url}) ({p.title.strip()} by {p.user.login})"
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
        type=int,
        help="Update the conflict comment and label for this pull request.",
        default=0,
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
        help="The comma-separated repo slugs of the remotes on GitHub.",
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
    for slug in args.github_repos.split(","):
        repo_dir = os.path.join(args.scratch_dir, slug)

        url = f"https://github.com/{slug}"
        if not os.path.isdir(repo_dir):
            print(f"Clone {url} repo to {repo_dir}")
            os.chdir(args.scratch_dir)
            call_git(["clone", "--quiet", url, repo_dir])
            print("Set git metadata")
            os.chdir(repo_dir)
            with open(os.path.join(repo_dir, ".git", "config"), "a") as f:
                f.write(f'[remote "{UPSTREAM_PULL}"]\n')
                f.write(f"    url = {url}\n")
                f.write(f"    fetch = +refs/pull/*:refs/remotes/upstream-pull/*\n")
                f.flush()
            call_git(["config", "user.email", "no@ne.nl"])
            call_git(["config", "user.name", "none"])
            call_git(["config", "gc.auto", "0"])

        print(f"Fetching diffs for {slug} ...")
        os.chdir(repo_dir)
        call_git(["fetch", "--quiet", "--all"])

        print(f"Fetching open pulls for {slug} ...")
        github_api = Github(args.github_access_token)
        github_repo = github_api.get_repo(slug)
        base_name = github_repo.default_branch
        pulls = return_with_pull_metadata(
            lambda: [p for p in github_repo.get_pulls(state="open", base=base_name)]
        )
        call_git(["fetch", "--quiet", "--all"])  # Do it again just to be safe
        call_git(["fetch", "origin", base_name, "--quiet"])

        print(f"Open {base_name}-pulls for {slug}: {len(pulls)}")
        pulls_mergeable = [p for p in pulls if p.mergeable]
        print(f"Open mergeable {base_name}-pulls for {slug}: {len(pulls_mergeable)}")

        with tempfile.TemporaryDirectory() as temp_git_work_tree:
            shutil.copytree(
                os.path.join(repo_dir, ".git"),
                os.path.join(temp_git_work_tree, ".git"),
            )
            os.chdir(temp_git_work_tree)

            if args.update_comments:
                for i, pull_update in enumerate(pulls_mergeable):
                    print(
                        f"{i}/{len(pulls_mergeable)} Checking for conflicts {base_name} <> {pull_update.number} <> other_pulls ... "
                    )
                    pulls_conflict = calc_conflicts(
                        pulls_mergeable=pulls_mergeable,
                        num=pull_update.number,
                        base_branch=base_name,
                    )
                    update_comment(
                        dry_run=args.dry_run,
                        pull=pull_update,
                        pulls_conflict=pulls_conflict,
                    )

            if args.pull_id:
                pull_merge = [p for p in pulls if p.number == args.pull_id]

                if not pull_merge:
                    print(
                        f"{args.pull_id} not found in all {len(pulls)} open {base_name} pulls"
                    )
                    sys.exit(-1)
                pull_merge = pull_merge[0]

                if not pull_merge.mergeable:
                    print(f"{pull_merge.number} is not mergeable")
                    sys.exit(-1)

                print(
                    f"Checking for conflicts {base_name} <> {pull_merge.number} <> other_pulls ... "
                )
                conflicts = calc_conflicts(
                    pulls_mergeable=pulls_mergeable,
                    num=pull_merge.number,
                    base_branch=base_name,
                )

                update_comment(
                    dry_run=args.dry_run, pull=pull_merge, pulls_conflict=conflicts
                )

            os.chdir(repo_dir)


if __name__ == "__main__":
    main()
