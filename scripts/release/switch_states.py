"""Materialize version installation and activation at their filesystem boundaries."""
import json
from pathlib import Path
import shutil

STEPS = [
    "partial download", "verified archive", "partial unpack", "complete staging tree",
    "synced staging tree", "renamed version", "synced versions directory", "highest-version write",
    "pending state temp", "pending state committed", "known-good temp", "known-good committed",
    "journal temp", "journal committed", "state temp", "state committed",
    "current temp", "current committed", "journal removed",
]


def materialize(root, old, new, step):
    # Before/after a sync can have the same visible names. Each recovery still runs.
    assert root.is_dir() and (root/"versions"/old/"bin/sunnad").is_file()
    phase = STEPS.index(step)
    def reached(name):
        return phase >= STEPS.index(name)
    versions = root/"versions"
    candidate = versions/new
    template = root/"fixture-payload"
    if not template.exists():
        shutil.copytree(candidate, template)
    stage = versions/(new+".staging")
    shutil.rmtree(stage, ignore_errors=True)
    if not reached("renamed version"):
        shutil.rmtree(candidate, ignore_errors=True)
    for name in ["transaction.new","transaction.json","current.new","state.new","known-good","known-good.new","highest"]:
        (root/name).unlink(missing_ok=True)
    cache = root/"downloads"
    cache.mkdir(exist_ok=True)
    for name in ["fixture.part","fixture.tar.gz"]:
        (cache/name).unlink(missing_ok=True)
    if step == "partial download":
        (cache/"fixture.part").write_bytes(b"partial")
    else:
        # Recovery never opens the archive. Only its directory entry matters here.
        (cache/"fixture.tar.gz").touch()
    if step == "partial unpack":
        (stage/"bin").mkdir(parents=True)
        (stage/"bin/sunnad").write_bytes(b"partial")
    if step in ("complete staging tree","synced staging tree"):
        shutil.copytree(template, stage)
    if reached("renamed version") and not candidate.exists():
        shutil.copytree(template, candidate)
    if reached("highest-version write"):
        (root/"highest").write_text(new)
    prior = dict(format=1,current=old,healthy=True,pending=new if reached("pending state committed") else None)
    after = dict(format=1,current=new,previous=old,healthy=False,request=None)
    def save(name, state):
        (root/name).write_text(json.dumps(state))
    if step == "pending state temp":
        save("state.new",dict(prior,pending=new))
    if step == "known-good temp":
        (root/"known-good.new").write_text(old)
    if reached("known-good committed"):
        (root/"known-good").write_text(old)
    if step == "journal temp":
        save("transaction.new",dict(after,before=prior))
    if reached("journal committed") and not reached("journal removed"):
        save("transaction.json",dict(after,before=prior))
    if step == "state temp":
        save("state.new",after)
    save("state.json",after if reached("state committed") else prior)
    if step == "current temp":
        (root/"current.new").symlink_to(Path("versions")/new)
    link=root/"current"
    link.unlink(missing_ok=True)
    link.symlink_to(Path("versions")/(new if reached("current committed") else old))
    return new if reached("journal committed") else old
