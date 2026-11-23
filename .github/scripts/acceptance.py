"""Milestone 3's acceptance on one system (issue #157), from the installed app.

With the app installed from a release's installer: the invented memory is created
with the `ipse` command the installer brought; the app opens it and starts `ipse ui
--app`, and stops it when it quits; Claude Desktop is connected with `ipse client add
--install claude-desktop`, and the server its configuration starts answers over MCP
with Ipse's tools. A screenshot of the app is saved beside the results.

Usage: acceptance.py <the app's program> <the ipse program> <folder for screenshots>
"""

import json
import os
import platform
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SYSTEM = platform.system()
IDENTIFIER = "io.github.loren844.ipse"


def step(text: str) -> None:
    print(f"\n== {text}", flush=True)


def app_settings() -> Path:
    """Where the app keeps which memory to open, as Tauri's app_config_dir."""
    home = Path.home()
    if SYSTEM == "Darwin":
        return home / "Library" / "Application Support" / IDENTIFIER
    if SYSTEM == "Windows":
        return Path(os.environ["APPDATA"]) / IDENTIFIER
    return Path(os.environ.get("XDG_CONFIG_HOME", home / ".config")) / IDENTIFIER


def serving() -> list[str]:
    """The command lines of the `ipse ui --app` processes running."""
    if SYSTEM == "Windows":
        out = subprocess.run(
            ["powershell", "-NoProfile", "-Command",
             "Get-CimInstance Win32_Process | ForEach-Object { $_.CommandLine }"],
            capture_output=True, text=True, check=False,
        ).stdout
    else:
        out = subprocess.run(["ps", "-eo", "args"], capture_output=True, text=True, check=False).stdout
    return [line for line in out.splitlines() if "ui" in line and "--app" in line and "ipse" in line.lower()]


def wait(condition, seconds: int) -> bool:
    for _ in range(seconds):
        if condition():
            return True
        time.sleep(1)
    return condition()


def screenshot(path: Path) -> None:
    """The whole screen, as the person would see it; nothing when the system has no tool."""
    if SYSTEM == "Darwin":
        command = ["screencapture", "-x", str(path)]
    elif SYSTEM == "Windows":
        script = (
            "Add-Type -AssemblyName System.Windows.Forms,System.Drawing;"
            "$b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds;"
            "$i=New-Object System.Drawing.Bitmap $b.Width,$b.Height;"
            "$g=[System.Drawing.Graphics]::FromImage($i);"
            "$g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size);"
            f"$i.Save('{path}')"
        )
        command = ["powershell", "-NoProfile", "-Command", script]
    else:
        command = ["import", "-window", "root", str(path)]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    print(f"screenshot {path.name}: {'saved' if result.returncode == 0 else result.stderr.strip()}")


def mcp_tools(config: dict) -> list[str]:
    """Starts the server as Claude Desktop would, from its configuration, and lists its tools."""
    server = config["mcpServers"]["ipse"]
    env = {**os.environ, **server.get("env", {})}
    messages = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "acceptance", "version": "1"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
    ]
    process = subprocess.Popen(
        [server["command"], *server["args"]], env=env, text=True,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    tools: list[str] = []
    try:
        for message in messages:
            process.stdin.write(json.dumps(message) + "\n")
            process.stdin.flush()
            if "id" not in message:
                continue
            answer = json.loads(process.stdout.readline())
            if "error" in answer:
                raise SystemExit(f"MCP error: {answer['error']}")
            if message["id"] == 2:
                tools = [tool["name"] for tool in answer["result"]["tools"]]
    finally:
        process.stdin.close()
        process.terminate()
        process.wait(timeout=10)
    return tools


def main() -> None:
    app, ipse, shots = sys.argv[1], sys.argv[2], Path(sys.argv[3])
    shots.mkdir(parents=True, exist_ok=True)
    failures = []

    step("The installed ipse command")
    print(subprocess.run([ipse, "--version"], capture_output=True, text=True, check=True).stdout.strip())

    step("The invented memory, made by the installed command")
    vault = Path(tempfile.mkdtemp()) / "Ipse demo"
    subprocess.run([ipse, "init", str(vault), "--demo", "--language", "fr",
                    "--time-zone", "Europe/Paris"], check=True, capture_output=True)
    settings = app_settings()
    settings.mkdir(parents=True, exist_ok=True)
    (settings / "memory.json").write_text(json.dumps({"folder": str(vault)}), encoding="utf-8")
    # Checking for updates is asked at the first start: answered no, nothing leaves.
    (settings / "app.json").write_text(json.dumps({"notifications": True, "updates": False}), encoding="utf-8")

    step("The app opens the memory")
    started = subprocess.Popen([app], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if wait(lambda: serving(), 90):
        print("ipse ui --app runs:", serving()[0])
    else:
        failures.append("the app did not start ipse ui --app")
    time.sleep(10)
    screenshot(shots / f"{SYSTEM.lower()}-home.png")

    step("Quitting the app stops the memory's server")
    started.terminate()
    try:
        started.wait(timeout=20)
    except subprocess.TimeoutExpired:
        started.kill()
    if SYSTEM == "Windows":
        # A terminated Windows process does not run its exit: the server is a child to end.
        subprocess.run(["taskkill", "/F", "/IM", "ipse.exe"], capture_output=True, check=False)
    if not wait(lambda: not serving(), 30):
        failures.append("ipse ui --app outlived the app")
    else:
        print("stopped")

    step("Claude Desktop, connected by the installed command")
    added = subprocess.run([ipse, "client", "add", "Claude", "--install", "claude-desktop",
                            "--vault", str(vault)], capture_output=True, text=True, check=False)
    print(added.stdout.strip() or added.stderr.strip())
    home = Path.home()
    config_path = {
        "Darwin": home / "Library" / "Application Support" / "Claude",
        "Windows": Path(os.environ.get("APPDATA", home / "AppData" / "Roaming")) / "Claude",
    }.get(SYSTEM, home / ".config" / "Claude") / "claude_desktop_config.json"
    if added.returncode != 0 or not config_path.exists():
        failures.append("ipse client add --install claude-desktop wrote no configuration")
    else:
        tools = mcp_tools(json.loads(config_path.read_text(encoding="utf-8")))
        print("tools:", ", ".join(tools))
        if "search" not in tools:
            failures.append(f"the MCP server Claude Desktop starts has no search tool: {tools}")

    if failures:
        print("\nFAILED:\n- " + "\n- ".join(failures))
        sys.exit(1)
    print("\nMilestone 3's acceptance passed on", SYSTEM)


if __name__ == "__main__":
    main()
