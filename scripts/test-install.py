"""Small isolated checks for installer boundaries; never installs on the test host."""
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

SCRIPT = Path(__file__).with_name("install.sh").resolve()


def shell(code, success=True):
    result = subprocess.run(["sh", "-c", f'. "{SCRIPT}"\n{code}'],
                            env={**os.environ, "LAYA_INSTALLER_TEST_SOURCE": "1"},
                            capture_output=True, text=True)
    assert (result.returncode == 0) == success, (code, result.returncode, result.stdout, result.stderr)
    return result


def main():
    shell('number 8080 65535; number 1324696149 2147483647; token_ok "abc.AZ_09-~/+=="; version_ok v0.1.0')
    for value in ("0", "08", "65536", "-1", "x", "999999999999999999999"):
        shell(f"number '{value}' 65535", False)
    for value in ("", "a=b", "=", "two words", "$(id)", "密钥"):
        shell(f"token_ok '{value}'", False)
    shell("version_ok '../outside'", False)
    shell("hash_ok 'aa'", False)
    # Noninteractive execution must fail before any Linux/root/filesystem check.
    result = subprocess.run(["sh", str(SCRIPT)], stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, start_new_session=True)
    assert result.returncode != 0 and "需要可交互终端" in result.stderr
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for name, member, kind in (("parent", "../escape", "file"),
                                   ("absolute", "/escape", "file"),
                                   ("link", "link", "link"), ("valid", "dir/file", "file")):
            path = root / f"{name}.tgz"
            with tarfile.open(path, "w:gz") as archive:
                info = tarfile.TarInfo(member)
                if kind == "link":
                    info.type = tarfile.SYMTYPE
                    info.linkname = "/etc"
                    archive.addfile(info)
                else:
                    info.size = 2
                    archive.addfile(info, io.BytesIO(b"ok"))
            shell(f'work="{root}"; unpack "{path}" "{root / name}"', name == "valid")
        assert (root / "valid/dir/file").read_text() == "ok"
        manifest = root / "release.env"
        manifest.write_text("VERSION=v1\nVERSION=v2\n")
        shell(f'field "{manifest}" VERSION', False)
        shell(f'field "{manifest}" MISSING', False)
        backup = root / "backup"
        backup.mkdir()
        (backup / "old-token.env").write_text("test-only-secret")
        shell(f'work="{backup}"; switched=1; rollback() {{ return 1; }}; trap cleanup EXIT; exit 1', False)
        assert (backup / "old-token.env").read_text() == "test-only-secret"
        stage = root / "partial-model"
        stage.mkdir()
        shell(f'model_stage="{stage}"; trap cleanup EXIT; exit 1', False)
        assert not stage.exists()
    # A manager reload/start failure must not be hidden by a later status success.
    shell('manager=systemd; systemctl() { case "$1" in daemon-reload) return 1;; *) return 0;; esac; }; if service_start; then exit 1; fi')
    shell('manager=systemd; systemctl() { case "$1" in enable) return 1;; *) return 0;; esac; }; if service_start; then exit 1; fi')
    print("PASS: validation, noninteractive refusal, archive traversal/link rejection, metadata and service failure propagation")


if __name__ == "__main__":
    main()
