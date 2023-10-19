import argparse
import subprocess

from util.util import call_git, get_git

def gen_coverage(docker_exec, dir_code, dir_result, git_ref, make_jobs):
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

    wrapper_dir = os.path.join(dir_code, 'wrappers')
    os.makedirs(wrapper_dir, exist_ok=True)
    with open(os.path.join(wrapper_dir, 'genhtml'), 'w') as f:
        f.write('#!/usr/bin/env bash\n')
        f.write('export LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1\n')
        f.write('export FAKETIME="2000-01-01 12:00:00"\n')
        f.write('/usr/bin/genhtml $@')
    docker_exec('chmod +x {}'.format(os.path.join(wrapper_dir, 'genhtml')))
    docker_exec('PATH={}:{} ../configure --enable-zmq --with-incompatible-bdb --enable-lcov --enable-lcov-branch-coverage --disable-bench'.format(wrapper_dir, '${PATH}'))
    docker_exec('make -j{}'.format(make_jobs))

    print('Make coverage ...')
    docker_exec('make cov')
    docker_exec('mv {}/*coverage* {}/'.format(dir_build, dir_result))
    os.chdir(dir_result)
    call_git(['checkout', 'main'])
    call_git(['add', './'])
    call_git(['commit', '-m', 'Add coverage results for {}'.format(git_ref)])
    call_git(['push', 'origin', 'main'])

    # Work around permission errors
    clear_dir(dir_result)
    os.chdir(dir_result)
    call_git(['reset', '--hard', 'HEAD'])

    return parse_perc(os.path.join(dir_result, 'total.coverage', 'index.html'))

def calc_coverage(base_ref, dir_code, dir_cov_report, make_jobs, slug, remote_url):
    print('Start docker process ...')
    os.makedirs(dir_cov_report, exist_ok=True)
    docker_id = subprocess.check_output(
        [
            'podman',
            'run',
            '-idt',
            '--rm',
            '--volume={}:{}:rw,z'.format(dir_code, dir_code),
            '--volume={}:{}:rw,z'.format(dir_cov_report, dir_cov_report),
            #'--mount', # Doesn't work with fedora (needs rw,z)
            #'type=bind,src={},dst={}'.format(dir_code, dir_code),
            #'--mount',
            #'type=bind,src={},dst={}'.format(dir_cov_report, dir_cov_report),
            '-e',
            'LC_ALL=C.UTF-8',
            'ubuntu:devel',  # Use latest lcov to avoid bugs in earlier versions
        ],
        universal_newlines=True,
    ).strip()

    docker_exec = lambda cmd: subprocess.check_output(['podman', 'exec', docker_id, 'bash', '-c', 'cd {} && {}'.format(os.getcwd(), cmd)], universal_newlines=True)

    print('Docker running with id {}.'.format(docker_id))

    print('Installing packages ...')
    docker_exec('apt-get update')
    docker_exec('apt-get install -qq {}'.format('ccache python3-zmq libsqlite3-dev libevent-dev libboost-system-dev libboost-filesystem-dev libboost-test-dev libdb5.3++-dev libminiupnpc-dev libzmq3-dev lcov build-essential libtool autotools-dev automake pkg-config bsdmainutils faketime'))

    print('Generate base coverage')
    os.chdir(dir_code)
    base_git_ref = get_git(['log', '--format=%H', '-1', base_ref])[:16]
    dir_result_base = os.path.join(dir_cov_report, f'{base_git_ref}')
    res_base = gen_coverage(docker_exec, dir_code, dir_result_base, base_git_ref, make_jobs)
    print(f'{remote_url}/coverage/{slug}/{base_git_ref}/total.coverage/index.html')

def main():
    parser = argparse.ArgumentParser(description='Run coverage reports for all pull requests.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--commit_only', help='Generate the coverage for this commit and exit.')
    parser.add_argument('--repo_code', help='The repo slug of the remote on GitHub for code.', default='bitcoin/bitcoin')
    parser.add_argument('--repo_report', help='The repo slug of the remote on GitHub for reports.', default='DrahtBot/reports')
    parser.add_argument('--remote_url', help='The remote url of the hosted html reports.', default='https://drahtbot.space/host_reports/DrahtBot/reports')
    parser.add_argument('--make_jobs', help='The number of make jobs.', default='2', type=int)
    parser.add_argument('--scratch_dir', help='The local dir used for scratching')
    parser.add_argument('--ssh_key', help='The ssh key for "repo_report"')
    args = parser.parse_args()

    args.scratch_dir = os.path.abspath(os.path.join(args.scratch_dir, ''))
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
    call_git(['checkout', 'main'])
    call_git(['reset', '--hard', 'origin/main'])

    if args.commit_only:
        os.chdir(code_dir)
        call_git(['fetch', 'origin', args.commit_only, '--quiet'])
        calc_coverage(base_ref=args.commit_only, dir_code=code_dir, dir_cov_report=os.path.join(report_dir, 'coverage', args.repo_code), make_jobs=args.make_jobs, slug=args.repo_code, remote_url=args.remote_url)
        return

if __name__ == '__main__':
    main()
