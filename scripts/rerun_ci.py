from github import Github
import argparse
import subprocess
import json

from util.util import return_with_pull_metadata


def rerun(*, task, dry_run):
    t_id = task["id"]
    raw_data = f"""
                    {{
                        "query":"mutation
                        {{
                           rerun(
                             input: {{
                               attachTerminal: false, clientMutationId: \\"rerun-{t_id}\\", taskId: \\"{t_id}\\"
                             }}
                           ) {{
                              newTask {{
                                id
                              }}
                           }}
                         }}"
                     }}
                 """
    print(f'Re-run task "{task["name"]}" (id: {t_id})')
    if not dry_run:
        subprocess.check_call(
            [
                "curl",
                "https://api.cirrus-ci.com/graphql",
                "-X",
                "POST",
                "-H",
                "Cookie: cirrusUserId=5103017154576384; cirrusAuthToken=dnsd12gbqgcne78r1hat70i9ko95u0abd8a15i",
                "--data-raw",
                raw_data,
            ],
            stderr=subprocess.DEVNULL,
        )
        print()


def main():
    parser = argparse.ArgumentParser(
        description="Trigger a CI run unconditionally.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--github_access_token", help="The access token for GitHub.", default=""
    )
    parser.add_argument(
        "--github_repos",
        help="The comma-separated list of repo slugs of the remotes on GitHub.",
        default="bitcoin-core/gui,bitcoin/bitcoin",
    )
    parser.add_argument(
        "--dry_run",
        help="Print changes/edits instead of calling the GitHub/CI API.",
        action="store_true",
        default=False,
    )
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    for slug in args.github_repos.split(","):
        github_repo = github_api.get_repo(slug)
        repo_owner, repo_name = slug.split("/")

        print(f"Get open pulls for {slug} ...")
        pulls = return_with_pull_metadata(
            lambda: [p for p in github_repo.get_pulls(state="open")]
        )

        print("Open pulls: {}".format(len(pulls)))

        for i, p in enumerate(pulls):
            print("{}/{}".format(i, len(pulls)))
            if p.mergeable:
                raw_data = f"""
                    {{
                        "query":"query
                        {{
                            githubRepository(owner: \\"{repo_owner}\\", name: \\"{repo_name}\\") {{
                              viewerPermission
                              builds(last: 1, branch: \\"pull/{p.number}\\") {{
                                edges {{
                                  node {{
                                    tasks {{
                                      id
                                      name
                                    }}
                                  }}
                                }}
                              }}
                            }}
                        }}"
                     }}
                """
                tasks = subprocess.check_output(
                    [
                        "curl",
                        "https://api.cirrus-ci.com/graphql",
                        "-X",
                        "POST",
                        "--data-raw",
                        raw_data,
                    ],
                    stderr=subprocess.DEVNULL,
                )
                tasks = json.loads(tasks)["data"]["githubRepository"]["builds"][
                    "edges"
                ][0]["node"]["tasks"]
                lint = [t for t in tasks if "lint" in t["name"]]
                prvr = [t for t in tasks if "previous releases" in t["name"]]
                if lint:
                    rerun(task=lint[0], dry_run=args.dry_run)
                if prvr:
                    rerun(task=prvr[0], dry_run=args.dry_run)


if __name__ == "__main__":
    main()
