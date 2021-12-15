from collections import defaultdict
from enum import Enum, unique
import re
import os
import subprocess

UPSTREAM_PULL = 'upstream-pull'


@unique
class IdComment(Enum):
    NEEDS_REBASE = '<!--cf906140f33d8803c4a75a2196329ecb-->'
    REVIEWERS_REQUESTED = '<!--4a62be1de6b64f3ed646cdc7932c8cf5-->'
    STALE = '<!--13523179cfe9479db18ec6c5d236f789-->'
    METADATA = '<!--e57a25ab6845829454e8d69fc972939a-->'  # The "root" section
    SEC_CONFLICTS = '<!--174a7506f384e20aa4161008e828411d-->'
    SEC_COVERAGE = '<!--2502f1a698b3751726fa55edcda76cd3-->'


def ensure_init_git(*, folder, url, add_upstream_pulls_remote=False, user_email='no@ne.nl', user_name='none'):
    if os.path.isdir(folder):
        return
    print(f'Clone {url} repo to {folder}')
    call_git(['clone', '--quiet', url, folder])
    print(f'Set git metadata for {folder}')
    os.chdir(folder)
    if add_upstream_pulls_remote:
        with open(os.path.join(folder, '.git', 'config'), 'a') as f:
            f.write('[remote "{}"]\n'.format(UPSTREAM_PULL))
            f.write('    url = {}\n'.format(url))
            f.write('    fetch = +refs/pull/*:refs/remotes/upstream-pull/*\n')
            f.flush()
    call_git(['config', 'user.email', user_email])
    call_git(['config', 'user.name', user_name])


def get_metadata_comment(sections):
    return ''.join([IdComment.METADATA.value + '\n\nThe following sections might be updated with supplementary metadata relevant to reviewers and maintainers.\n\n'] + sorted(sections))


def get_metadata_sections(pull):
    for c in pull.get_issue_comments():
        if c.body.startswith(IdComment.METADATA.value):
            sections = ['<!--' + s for s in c.body.split('<!--')][2:]
            return c, sections
    return None, None


def update_metadata_comment(pull, section_id, text, dry_run):
    c, sections = get_metadata_sections(pull)
    if sections is not None:
        for i in range(len(sections)):
            if sections[i].startswith(section_id):
                # Section exists
                if sections[i].split('-->', 1)[1] == text:
                    # Section up to date
                    return
                # Update section
                sections[i] = section_id + text
                text_all = get_metadata_comment(sections)
                print('{}\n    .{}\n        .body = {}\n'.format(pull, c, text_all))
                if not dry_run:
                    c.edit(text_all)
                return
        # Create new section
        text_all = get_metadata_comment(sections + [section_id + text])
        print('{}\n    .{}\n        .body = {}\n'.format(pull, c, text_all))
        if not dry_run:
            c.edit(text_all)
        return
    # Create new metadata comment
    text_all = get_metadata_comment([section_id + text])
    print('{}\n    .new_comment.body = {}\n'.format(pull, text_all))
    if not dry_run:
        pull.create_issue_comment(text_all)
    return


def get_section_text(pull, section_id):
    _, sections = get_metadata_sections(pull)
    if sections:
        for s in sections:
            if s.startswith(section_id):
                return s.split('-->', 1)[1]
    return None


def return_with_pull_metadata(get_pulls):
    pulls = get_pulls()
    pulls_update_mergeable = lambda: [p for p in pulls if p.mergeable is None and not p.merged]
    print('Fetching open pulls metadata ...')
    while pulls_update_mergeable():
        print('Update mergable state for pulls {}'.format([p.number for p in pulls_update_mergeable()]))
        [p.update() for p in pulls_update_mergeable()]
        pulls = get_pulls()
    return pulls


def calculate_table(base_folder, commit_folder, external_url, base_commit, commit):
    rows = defaultdict(lambda: ['', ''])  # map from abbrev file name to list of links
    for f in sorted(os.listdir(base_folder)):
        short_file_name = re.sub(r'(bitcoin-)?[a-f0-9]{12}', '*', f)
        os.chdir(base_folder)
        left = rows[short_file_name]
        left[0] = '[`{}...`]({}{}/{})'.format(subprocess.check_output(['sha256sum', f], universal_newlines=True)[:16], external_url, base_commit, f)
        rows[short_file_name] = left

    for f in sorted(os.listdir(commit_folder)):
        short_file_name = re.sub(r'(bitcoin-)?[a-f0-9]{12}', '*', f)
        os.chdir(commit_folder)
        right = rows[short_file_name]
        right[1] = '[`{}...`]({}{}/{})'.format(subprocess.check_output(['sha256sum', f], universal_newlines=True)[:16], external_url, commit, f)
        rows[short_file_name] = right

    text = ''
    for f in rows:
        text += '| {} | {} | {} |\n'.format(f, rows[f][0], rows[f][1])
    text += '\n'
    return text


def call_git(args, **kwargs):
    subprocess.check_call(['git'] + args, **kwargs)


def get_git(args):
    return subprocess.check_output(['git'] + args, universal_newlines=True).strip()
