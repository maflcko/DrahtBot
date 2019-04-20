#!/usr/bin/env python3
# Copyright (c) 2014-2018 The Bitcoin Core developers
# Distributed under the MIT software license, see the accompanying
# file COPYING or http://www.opensource.org/licenses/mit-license.php.

from github import Github, GithubException
import time
import datetime
import os
import sys
import shutil
import argparse
import collections
import subprocess

from util.util import return_with_pull_metadata, call_git, get_git, IdComment, update_metadata_comment, get_section_text

ID_COVERAGE_SEC = IdComment.SEC_COVERAGE.value
CovResult = collections.namedtuple('Result', ['lin', 'fun', 'bra'])

UPSTREAM_PULL = 'upstream-pull'


def parse_perc(html_path):
    parsed_lines = 0
    parsed_fun = 0
    parsed_branch = 0
    with open(html_path, encoding='utf-8') as f:
        for l in f:
            if '_coverage.info</td>' in l:
                bench_name_base = l.split('_coverage.info</td>')[0].split('>')[-1]
                continue

            if '>Lines:</td>' in l:
                parsed_lines = 1
                continue
            if parsed_lines == 1:
                hit_lines = int(l.split('</td>')[0].split('>')[-1])
                parsed_lines += 1
                continue
            if parsed_lines == 2:
                total_lines = int(l.split('</td>')[0].split('>')[-1])
                parsed_lines += 1
                continue

            if '>Functions:</td>' in l:
                parsed_fun = 1
                continue
            if parsed_fun == 1:
                hit_fun = int(l.split('</td>')[0].split('>')[-1])
                parsed_fun += 1
                continue
            if parsed_fun == 2:
                total_fun = int(l.split('</td>')[0].split('>')[-1])
                parsed_fun += 1
                continue

            if '>Branches:</td>' in l:
                parsed_branch = 1
                continue
            if parsed_branch == 1:
                hit_branch = int(l.split('</td>')[0].split('>')[-1])
                parsed_branch += 1
                continue
            if parsed_branch == 2:
                total_branch = int(l.split('</td>')[0].split('>')[-1])
                parsed_branch += 1
                continue

            if parsed_branch == 2 and parsed_fun == 2 and parsed_lines == 2:
                break

    return CovResult(
        lin=100. * hit_lines / total_lines,
        fun=100. * hit_fun / total_fun,
        bra=100. * hit_branch / total_branch,
    )


def gen_coverage(docker_exec, dir_code, dir_result, git_ref, make_jobs, *, cache_base=False):
    print('Generate coverage for {} in {} (ref: {}).'.format(dir_code, dir_result, git_ref))
    os.chdir(dir_code)
    dir_build = os.path.join(dir_code, 'build')
    dir_cache = os.path.join(dir_code, 'cache_base')

    print('Clear previous build and result folders')

    def clear_dir(folder):
        os.makedirs(folder, exist_ok=True)
        docker_exec('rm -r {}'.format(folder))
        os.makedirs(folder, exist_ok=True)
        # Must change to a dir that exists after this function call

    clear_dir(dir_build)
    clear_dir(dir_result)
    clear_dir(dir_cache) if cache_base else None

    print('Make coverage data in docker ...')
    os.chdir(dir_code)
    call_git(['checkout', '{}'.format(git_ref)])
    docker_exec('./autogen.sh')
    os.chdir(dir_build)

    wrapper_dir = os.path.join(dir_code, 'wrappers')
    os.makedirs(wrapper_dir, exist_ok=True)
    with open(os.path.join(wrapper_dir, 'genhtml'), 'w') as f:
        f.write('#!/usr/bin/env bash\n')
        f.write('export LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1\n')
        f.write('export FAKETIME="2000-01-01 12:00:00"\n')
        f.write('/usr/bin/genhtml $@')
    docker_exec('chmod +x {}'.format(os.path.join(wrapper_dir, 'genhtml')))
    docker_exec('PATH={}:{} ../configure --enable-zmq --with-incompatible-bdb --enable-lcov --enable-lcov-branch-coverage --disable-bench'.format(wrapper_dir, '${PATH}'))
    if cache_base:
        print('Cache compiled obj files of {} in {} ...'.format(git_ref, dir_cache))
        docker_exec('make -j{}'.format(make_jobs))
        docker_exec('rmdir {}'.format(dir_cache))
        docker_exec('mv {} {}'.format(dir_build, dir_cache))

    print('Restore compiled obj files from cache ...')
    clear_dir(dir_build)
    os.chdir(dir_cache)  # Change to a dir that exists
    docker_exec('rmdir {}'.format(dir_build))
    docker_exec('cp -r {} {}'.format(dir_cache, dir_build))
    print('re-make ...')
    os.chdir(dir_build)
    docker_exec('make -j{}'.format(make_jobs))

    print('Make coverage ...')
    docker_exec('make cov')
    docker_exec('mv {}/*coverage* {}/'.format(dir_build, dir_result))
    os.chdir(dir_result)
    call_git(['checkout', 'master'])
    call_git(['add', './'])
    call_git(['commit', '-m', 'Add coverage results for {}'.format(git_ref)])
    call_git(['push', 'origin', 'master'])

    # Work around permission errors
    clear_dir(dir_result)
    os.chdir(dir_result)
    call_git(['reset', '--hard', 'HEAD'])

    return parse_perc(os.path.join(dir_result, 'total.coverage', 'index.html'))


def pull_needs_update(pull):
    text = get_section_text(pull, ID_COVERAGE_SEC)
    if not text:
        return True

    updated_at = text.split('<sup>Updated at: ', 1)[1].split('.</sup>', 1)[0]
    updated_at = datetime.datetime.fromisoformat(updated_at)
    delta = datetime.datetime.utcnow() - updated_at
    return delta > datetime.timedelta(days=3)


def calc_coverage(pulls, base_branch, dir_code, dir_cov_report, make_jobs, dry_run, slug, remote_url):
    print('Start docker process ...')
    os.makedirs(dir_cov_report, exist_ok=True)
    docker_id = subprocess.check_output([
        'podman',
        'run',
        '-idt',
        #'--rm', # Doesn't work with podman
        '--volume={}:{}:rw,z'.format(dir_code, dir_code),
        '--volume={}:{}:rw,z'.format(dir_cov_report, dir_cov_report),
        #'--mount', # Doesn't work with fedora (needs rw,z)
        #'type=bind,src={},dst={}'.format(dir_code, dir_code),
        #'--mount',
        #'type=bind,src={},dst={}'.format(dir_cov_report, dir_cov_report),
        '-e',
        'LC_ALL=C.UTF-8',
        'ubuntu:18.04',
    ], universal_newlines=True).strip()

    docker_exec = lambda cmd: subprocess.check_output(['podman', 'exec', docker_id, 'bash', '-c', 'cd {} && {}'.format(os.getcwd(), cmd)], universal_newlines=True)

    print('Docker running with id {}.'.format(docker_id))

    print('Installing packages ...')
    docker_exec('apt-get update')
    docker_exec('apt-get install -qq {}'.format('python3-zmq libssl-dev libevent-dev libboost-system-dev libboost-filesystem-dev libboost-chrono-dev libboost-test-dev libboost-thread-dev libdb5.3++-dev libminiupnpc-dev libzmq3-dev lcov build-essential libtool autotools-dev automake pkg-config bsdmainutils faketime'))

    print('Generate base coverage')
    os.chdir(dir_code)
    base_git_ref = get_git(['log', '--format=%H', '-1', base_branch])
    dir_result_base = os.path.join(dir_cov_report, '{}'.format(base_branch))
    res_base = gen_coverage(docker_exec, dir_code, dir_result_base, base_git_ref, make_jobs, cache_base=True)

    for i, pull in enumerate(pulls):
        print('{}/{} Calculating coverage ... '.format(i, len(pulls)))
        if not pull_needs_update(pull):
            continue
        os.chdir(dir_code)
        pull_git_ref = get_git(['log', '--format=%H', '-1', '{}/{}/merge'.format(UPSTREAM_PULL, pull.number)])
        dir_result_pull = os.path.join(dir_cov_report, '{}'.format(pull.number))
        res_pull = gen_coverage(docker_exec, dir_code, dir_result_pull, pull_git_ref, make_jobs)
        text = '\n\n### Coverage\n'
        text += '\n'
        text += '| Coverage  | Change ([pull {pull_id}]({url_pull}), {pull_git_ref}) | Reference ([{base_name}]({url_base}), {base_git_ref})   |\n'
        text += '|-----------|-------------------------|--------------------|\n'
        text += '| Lines     | {p_l:+.4f} %            | {m_l:.4f} %        |\n'
        text += '| Functions | {p_f:+.4f} %            | {m_f:.4f} %        |\n'
        text += '| Branches  | {p_b:+.4f} %            | {m_b:.4f} %        |\n'
        text += '\n<sup>Updated at: {updated_at}.</sup>\n'
        text = text.format(
            url_base='{}/{}/{}/{}/total.coverage/index.html'.format(remote_url, 'coverage', slug, base_branch),
            url_pull='{}/{}/{}/{}/total.coverage/index.html'.format(remote_url, 'coverage', slug, pull.number),
            base_name=base_branch,
            pull_id=pull.number,
            p_l=res_pull.lin - res_base.lin,
            p_f=res_pull.fun - res_base.fun,
            p_b=res_pull.bra - res_base.bra,
            m_l=res_base.lin,
            m_f=res_base.fun,
            m_b=res_base.bra,
            base_git_ref=base_git_ref,
            pull_git_ref=pull_git_ref,
            updated_at=datetime.datetime.utcnow().isoformat(),
        )
        update_metadata_comment(pull, ID_COVERAGE_SEC, text=text, dry_run=dry_run)


def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(description='Run coverage reports for all pull requests.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--pull_id', type=int, help='Update the comment for this pull request.', default=0)
    parser.add_argument('--update_comments', action='store_true', help='Update all comments.', default=False)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--repo_code', help='The repo slug of the remote on GitHub for code.', default='bitcoin/bitcoin')
    parser.add_argument('--repo_report', help='The repo slug of the remote on GitHub for reports.', default='DrahtBot/reports')
    parser.add_argument('--remote_url', help='The remote url of the hosted html reports.', default='https://drahtbot.github.io/reports')
    parser.add_argument('--make_jobs', help='The number of make jobs.', default='2', type=int)
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    parser.add_argument('--scratch_dir', help='The local dir used for scratching', default=os.path.join(THIS_FILE_PATH, '..', 'scratch_coverage'))
    parser.add_argument('--ssh_key', help='The ssh key for "repo_report"', default=os.path.join(THIS_FILE_PATH, '..', 'ssh_env', 'id_rsa_drahtbot'))
    parser.add_argument('--base_name', help='The name of the base branch.', default='master')
    args = parser.parse_args()

    args.scratch_dir = os.path.normpath(os.path.join(args.scratch_dir, ''))
    os.makedirs(args.scratch_dir, exist_ok=True)

    code_dir = os.path.join(args.scratch_dir, 'code', args.repo_code)
    code_url = 'https://github.com/{}'.format(args.repo_code)
    report_dir = os.path.join(args.scratch_dir, 'reports')
    report_url = 'git@github.com:{}.git'.format(args.repo_report)

    def create_scratch_dir(folder, url):
        if os.path.isdir(folder):
            return
        print('Clone {} repo to {}'.format(url, folder))
        os.chdir(args.scratch_dir)
        call_git(['clone', '--quiet', url, folder])
        print('Set git metadata')
        os.chdir(folder)
        with open(os.path.join(folder, '.git', 'config'), 'a') as f:
            f.write('[remote "{}"]\n'.format(UPSTREAM_PULL))
            f.write('    url = {}\n'.format(url))
            f.write('    fetch = +refs/pull/*:refs/remotes/upstream-pull/*\n')
            f.flush()
        call_git(['config', 'user.email', '39886733+DrahtBot@users.noreply.github.com'])
        call_git(['config', 'user.name', 'DrahtBot'])
        call_git(['config', 'core.sshCommand', 'ssh -i {} -F /dev/null'.format(args.ssh_key)])

    create_scratch_dir(code_dir, code_url)
    create_scratch_dir(report_dir, report_url)

    print('Fetching diffs ...')
    os.chdir(code_dir)
    call_git(['fetch', '--quiet', '--all'])
    call_git(['reset', '--hard', 'HEAD'])
    call_git(['checkout', 'master'])
    call_git(['pull', '--ff-only', 'origin', 'master'])
    os.chdir(report_dir)
    call_git(['fetch', '--quiet', '--all'])
    call_git(['reset', '--hard', 'HEAD'])
    call_git(['checkout', 'master'])
    call_git(['reset', '--hard', 'origin/master'])

    print('Fetching open pulls ...')
    github_api = Github(args.github_access_token)
    repo_code = github_api.get_repo(args.repo_code)
    pulls = return_with_pull_metadata(lambda: [p for p in repo_code.get_pulls(state='open')][:9])
    call_git(['fetch', '--quiet', '--all'])  # Do it again just to be safe
    call_git(['fetch', 'origin', '{}'.format(args.base_name), '--quiet'])
    pulls = [p for p in pulls if p.base.ref == args.base_name]

    print('Open {}-pulls: {}'.format(args.base_name, len(pulls)))
    pulls_mergeable = [p for p in pulls if p.mergeable]
    print('Open mergeable {}-pulls: {}'.format(args.base_name, len(pulls_mergeable)))

    if args.update_comments:
        calc_coverage(pulls=pulls_mergeable, base_branch=args.base_name, dir_code=code_dir, dir_cov_report=os.path.join(report_dir, 'coverage', args.repo_code), make_jobs=args.make_jobs, dry_run=args.dry_run, slug=args.repo_code, remote_url=args.remote_url)

    if args.pull_id:
        pull_update_id = [p for p in pulls if p.number == args.pull_id]

        if not pull_update_id:
            print('{} not found in all {} open {} pulls'.format(args.pull_id, len(pulls), args.base_name))
            sys.exit(-1)
        pull_update_id = pull_update_id[0]

        if not pull_update_id.mergeable:
            print('{} is not mergeable'.format(pull_update_id.number))
            sys.exit(-1)

        calc_coverage(pulls=[pull_update_id], base_branch=args.base_name, dir_code=code_dir, dir_cov_report=os.path.join(report_dir, 'coverage', args.repo_code), make_jobs=args.make_jobs, dry_run=args.dry_run, slug=args.repo_code, remote_url=args.remote_url)


if __name__ == '__main__':
    main()
