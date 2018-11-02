import subprocess
from enum import Enum, unique


@unique
class IdComment(Enum):
    NEEDS_REBASE = '<!--cf906140f33d8803c4a75a2196329ecb-->'
    CLOSED = '<!--5fd3d806e98f4a0ca80977bb178665a0-->'
    METADATA = '<!--e57a25ab6845829454e8d69fc972939a-->'  # The "root" section
    SEC_CONFLICTS = '<!--174a7506f384e20aa4161008e828411d-->'


def get_metadata_comment(sections):
    return ''.join([IdComment.METADATA.value + '\n\nThe following sections might be updated with supplementary metadata relevant to reviewers and maintainers.\n\n'] + sorted(sections))


def update_metadata_comment(pull, section_id, text, dry_run):
    for c in pull.get_issue_comments():
        if c.body.startswith(IdComment.METADATA.value):
            sections = ['<!--' + s for s in c.body.split('<!--')][2:]
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
    if pull.number < 14618:
        return  # for now don't add the comment to grandfathered-in pulls
    text_all = get_metadata_comment([section_id + text])
    print('{}\n    .new_comment.body = {}\n'.format(pull, text_all))
    if not dry_run:
        pull.create_issue_comment(text_all)
    return


def return_with_pull_metadata(get_pulls):
    pulls = get_pulls()
    pulls_update_mergeable = lambda: [p for p in pulls if p.mergeable is None and not p.merged]
    print('Fetching open pulls metadata ...')
    while pulls_update_mergeable():
        print('Update mergable state for pulls {}'.format([p.number for p in pulls_update_mergeable()]))
        [p.update() for p in pulls_update_mergeable()]
        pulls = get_pulls()
    return pulls


def call_git(args, **kwargs):
    subprocess.check_call(['git'] + args, **kwargs)


def get_git(args):
    return subprocess.check_output(['git'] + args, universal_newlines=True).strip()
