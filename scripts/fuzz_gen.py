import argparse
import os
import sys
import subprocess
import shutil

from util.util import call_git


def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(description='Generate fuzz seeds until a crash.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--scratch_folder', help='The local scratch folder', default=os.path.join(THIS_FILE_PATH, '..', 'scratch', 'fuzz_gen'))
    parser.add_argument('--jobs', help='The number of jobs', default=1)
    args = parser.parse_args()

    print('''
    To prepare, install:
    sed git ccache llvm + Bitcoin Core deps
    ''')

    url_code = 'https://github.com/{}'.format('bitcoin/bitcoin')
    url_seed = 'https://github.com/{}'.format('bitcoin-core/qa-assets')
    temp_dir = os.path.normpath(os.path.join(args.scratch_folder, ''))
    dir_code = os.path.join(temp_dir, 'code')
    dir_assets = os.path.join(temp_dir, 'assets')
    dir_generate_seeds = os.path.join(temp_dir, 'generate_seeds')

    if not os.path.isdir(dir_code):
        print('Clone {} repo to {}'.format(url_code, dir_code))
        call_git(['clone', '--quiet', url_code, dir_code])
    if not os.path.isdir(dir_assets):
        print('Clone {} repo to {}'.format(url_seed, dir_assets))
        call_git(['clone', '--quiet', url_seed, dir_assets])

    while True:
        print('Fetch upsteam, checkout latest branch')
        os.chdir(dir_code)
        call_git(['fetch', '--quiet', '--all'])
        call_git(['checkout', 'origin/master'])
        call_git(['reset', '--hard', 'HEAD'])
        call_git(['clean', '-dfx'])
        subprocess.check_call(['sed', '-i', 's/runs=100000/use_value_profile=1","-max_total_time=600/g', 'test/fuzz/test_runner.py'])

        os.chdir(dir_assets)
        call_git(['fetch', '--quiet', '--all'])
        call_git(['checkout', 'origin/master'])

        os.chdir(dir_code)
        subprocess.check_call(f'./autogen.sh && CC=clang CXX=clang++ ./configure --enable-fuzz --with-sanitizers=address,fuzzer,undefined && make clean && make -j {args.jobs}', shell=True)
        shutil.rmtree(dir_generate_seeds)
        subprocess.check_call([sys.executable, 'test/fuzz/test_runner.py', '-l=DEBUG', f'--par={args.jobs}', f'{dir_generate_seeds}', f'--m_dir={dir_assets}/fuzz_seed_corpus'])
        subprocess.check_call([sys.executable, 'test/fuzz/test_runner.py', '-l=DEBUG', f'--par={args.jobs}', f'{dir_generate_seeds}', '--generate'])
        subprocess.check_call([sys.executable, 'test/fuzz/test_runner.py', '-l=DEBUG', f'--par={args.jobs}', f'{dir_assets}/fuzz_seed_corpus', f'--m_dir={dir_generate_seeds}'])


if __name__ == '__main__':
    main()
