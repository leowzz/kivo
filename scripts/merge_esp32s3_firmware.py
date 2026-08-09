from pathlib import Path
import subprocess

Import("env")


def merge_factory_image(source, target, env):
    build_dir = Path(env.subst("$BUILD_DIR"))
    program_name = env.subst("$PROGNAME")
    firmware = build_dir / f"{program_name}.bin"
    output = build_dir / f"{program_name}.factory.bin"
    board = env.BoardConfig()
    images = [
        (offset, Path(env.subst(path)))
        for offset, path in env.get("FLASH_EXTRA_IMAGES", [])
    ]
    images.append((env.subst("$ESP32_APP_OFFSET"), firmware))

    command = [
        env.subst("$PYTHONEXE"),
        env.subst("$OBJCOPY"),
        "--chip",
        board.get("build.mcu"),
        "merge_bin",
        "--flash_mode",
        "keep",
        "--flash_freq",
        "keep",
        "--flash_size",
        "keep",
        "-o",
        str(output),
    ]
    for offset, path in images:
        command.extend((str(offset), str(path)))

    subprocess.run(command, check=True)


if env.subst("$PIOENV") == "esp32s3":
    env.AddPostAction(
        "$BUILD_DIR/${PROGNAME}.bin",
        env.VerboseAction(
            merge_factory_image,
            "Building $BUILD_DIR/${PROGNAME}.factory.bin",
        ),
    )
