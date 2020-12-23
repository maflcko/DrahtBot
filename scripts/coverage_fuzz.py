from github import Github, GithubException
import time
import datetime
import os
import sys
import shutil
import argparse
import collections
import subprocess

from util.util import call_git, get_git

UPSTREAM_PULL = 'upstream-pull'


def gen_coverage(docker_exec, assets_dir, dir_code, dir_result, git_ref, make_jobs):
    print('Generate coverage for {} in {} (ref: {}).'.format(dir_code, dir_result, git_ref))
    os.chdir(dir_code)
    dir_build = os.path.join(dir_code, 'build')

    print('Clear previous build and result folders')

    def clear_dir(folder):
        os.makedirs(folder, exist_ok=True)
        docker_exec('rm -r {}'.format(folder))
        os.makedirs(folder, exist_ok=True)
        # Must change to a dir that exists after this function call

    clear_dir(dir_build)
    clear_dir(dir_result)

    print('Make coverage data in docker ...')
    os.chdir(dir_code)
    call_git(['checkout', '{}'.format(git_ref)])
    docker_exec('./autogen.sh')
    os.chdir(dir_build)

    docker_exec('../configure --enable-fuzz --with-sanitizers=fuzzer --enable-lcov --enable-lcov-branch-coverage CC=clang CXX=clang++')
    docker_exec('make -j{}'.format(make_jobs))

    print('Make coverage ...')
    docker_exec(f'make cov_fuzz DIR_FUZZ_SEED_CORPUS={assets_dir}/fuzz_seed_corpus')
    docker_exec('mv {}/*coverage* {}/'.format(dir_build, dir_result))  # TODO need to overwrite?
    os.chdir(dir_result)
    call_git(['checkout', 'main'])
    call_git(['add', './'])
    call_git(['commit', '-m', 'Add fuzz coverage results for {}'.format(git_ref)])
    call_git(['push', 'origin', 'main'])

    # Work around permission errors
    clear_dir(dir_result)
    os.chdir(dir_result)
    call_git(['reset', '--hard', 'HEAD'])


def calc_coverage(assets_dir, dir_code, dir_cov_report, make_jobs, args):
    print('Start docker process ...')
    os.makedirs(dir_cov_report, exist_ok=True)
    docker_id = subprocess.check_output(
        [
            'podman',
            'run',
            '-idt',
            '--rm',
            '--volume={}:{}:rw,z'.format(assets_dir, assets_dir),
            '--volume={}:{}:rw,z'.format(dir_code, dir_code),
            '--volume={}:{}:rw,z'.format(dir_cov_report, dir_cov_report),
            #'--mount', # Doesn't work with fedora (needs rw,z)
            #'type=bind,src={},dst={}'.format(dir_code, dir_code),
            #'--mount',
            #'type=bind,src={},dst={}'.format(dir_cov_report, dir_cov_report),
            '-e',
            'LC_ALL=C.UTF-8',
            'debian:bullseye-slim',  # Use debian 11 for lcov 1.14
        ],
        universal_newlines=True,
    ).strip()

    docker_exec = lambda cmd: subprocess.check_output(['podman', 'exec', docker_id, 'bash', '-c', 'cd {} && {}'.format(os.getcwd(), cmd)], universal_newlines=True)

    print('Docker running with id {}.'.format(docker_id))

    print('Installing packages ...')
    docker_exec('apt-get update')
    docker_exec('apt-get install -qq {}'.format('clang llvm ccache python3-zmq libssl-dev libsqlite3-dev libevent-dev libboost-system-dev libboost-filesystem-dev libboost-test-dev libboost-thread-dev libdb5.3++-dev libminiupnpc-dev libzmq3-dev lcov build-essential libtool autotools-dev automake pkg-config bsdmainutils faketime'))

    print('Generate coverage')
    os.chdir(dir_code)
    base_git_ref = get_git(['log', '--format=%H', '-1', 'HEAD'])[:16]
    dir_result_base = os.path.join(dir_cov_report, f'{base_git_ref}')
    gen_coverage(docker_exec, assets_dir, dir_code, dir_result_base, base_git_ref, make_jobs)

    print(f'{args.remote_url}/coverage_fuzz/monotree/{base_git_ref}/fuzz.coverage/index.html')


def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(description='Run fuzz coverage reports for one fuzz target.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--repo_report', help='The repo slug of the remote on GitHub for reports.', default='DrahtBot/reports')
    parser.add_argument('--remote_url', help='The remote url of the hosted html reports.', default='https://drahtbot.space/host_reports/DrahtBot/reports')
    parser.add_argument('--make_jobs', help='The number of make jobs.', default='2', type=int)
    parser.add_argument('--scratch_dir', help='The local dir used for scratching', default=os.path.join(THIS_FILE_PATH, '..', 'scratch', 'coverage_fuzz'))
    parser.add_argument('--ssh_key', help='The ssh key for "repo_report"', default=os.path.join(THIS_FILE_PATH, '..', 'ssh_env', 'id_rsa_drahtbot'))
    parser.add_argument('--git_ref_code', help='Which git ref in the code repo to build.', default='origin/master')
    parser.add_argument('--git_ref_qa_assets', help='Which git ref in the qa-assets repo to use.', default='origin/master')
    parser.add_argument('--fuzz_targets', help='Which targets to build.', default='')
    args = parser.parse_args()

    args.scratch_dir = os.path.normpath(os.path.join(args.scratch_dir, ''))
    os.makedirs(args.scratch_dir, exist_ok=True)

    code_dir = os.path.join(args.scratch_dir, 'code', 'monotree')
    code_url = 'https://github.com/MarcoFalke/bitcoin-core'
    report_dir = os.path.join(args.scratch_dir, 'reports')
    report_url = 'git@github.com:{}.git'.format(args.repo_report)
    assets_dir = os.path.join(args.scratch_dir, 'assets')
    assets_url = 'https://github.com/bitcoin-core/qa-assets'

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
    create_scratch_dir(assets_dir, assets_url)

    print('Fetching diffs ...')
    os.chdir(code_dir)
    call_git(['fetch', 'origin', '--quiet', args.git_ref_code])
    call_git(['checkout', args.git_ref_code, '--force'])
    call_git(['reset', '--hard', 'HEAD'])
    call_git(['clean', '-dfx'])
    subprocess.check_call(['sed', '-i', f's/DIR_FUZZ_SEED_CORPUS) -l DEBUG/DIR_FUZZ_SEED_CORPUS) {args.fuzz_targets} -l DEBUG/g', 'Makefile.am'])
    os.chdir(report_dir)
    call_git(['fetch', '--quiet', '--all'])
    call_git(['reset', '--hard', 'HEAD'])
    call_git(['checkout', 'main'])
    call_git(['reset', '--hard', 'origin/main'])
    os.chdir(assets_dir)
    call_git(['fetch', 'origin', '--quiet', args.git_ref_qa_assets])
    call_git(['checkout', args.git_ref_qa_assets])
    call_git(['clean', '-dfx'])

    calc_coverage(assets_dir=assets_dir, dir_code=code_dir, dir_cov_report=os.path.join(report_dir, 'coverage_fuzz', 'monotree'), make_jobs=args.make_jobs, args=args)


if __name__ == '__main__':
    main()
