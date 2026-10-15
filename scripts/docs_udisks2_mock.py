"""A python-dbusmock template for UDisks2: removable drives with one mounted filesystem each.

Loaded by scripts/docs-screenshots.py on a private system bus. Parameters:
    {"drives": [{"name": "sdb", "vendor": "SanDisk", "model": "Ultra USB 3.0",
                 "size": 64000000000, "label": "TRAVEL", "mount": "/path"}]}
"""

import dbus
from dbusmock.mockobject import objects

BUS_NAME = "org.freedesktop.UDisks2"
MAIN_OBJ = "/org/freedesktop/UDisks2"
MAIN_IFACE = "org.freedesktop.UDisks2.Manager"
SYSTEM_BUS = True
IS_OBJECT_MANAGER = True

DRIVE = "org.freedesktop.UDisks2.Drive"
BLOCK = "org.freedesktop.UDisks2.Block"
FILESYSTEM = "org.freedesktop.UDisks2.Filesystem"


def path_bytes(text):
    return dbus.Array([dbus.Byte(byte) for byte in text.encode() + b"\0"], signature="y")


def load(mock, parameters):
    mock.AddProperties(MAIN_IFACE, {"Version": "2.10.1"})
    for drive in parameters.get("drives", []):
        name = drive["name"]
        drive_path = f"/org/freedesktop/UDisks2/drives/{name}"
        block_path = f"/org/freedesktop/UDisks2/block_devices/{name}1"
        mock.AddObject(
            drive_path,
            DRIVE,
            {
                "Vendor": drive["vendor"],
                "Model": drive["model"],
                "Serial": "",
                "Revision": "",
                "Size": dbus.UInt64(drive["size"]),
                "Media": "thumb",
                "MediaAvailable": True,
                "MediaRemovable": True,
                "Removable": True,
                "Ejectable": True,
                "CanPowerOff": True,
                "ConnectionBus": "usb",
                "Optical": False,
                "SortKey": name,
            },
            [
                ("Eject", "a{sv}", "", ""),
                ("PowerOff", "a{sv}", "", ""),
            ],
        )
        mock.AddObject(
            block_path,
            BLOCK,
            {
                "Device": path_bytes(f"/dev/{name}1"),
                "PreferredDevice": path_bytes(f"/dev/{name}1"),
                "Drive": dbus.ObjectPath(drive_path),
                "CryptoBackingDevice": dbus.ObjectPath("/"),
                "IdLabel": drive["label"],
                "IdType": "exfat",
                "IdUUID": "",
                "IdUsage": "filesystem",
                "Size": dbus.UInt64(drive["size"]),
                "ReadOnly": False,
                "HintAuto": True,
                "HintIgnore": False,
                "HintSystem": False,
                "HintName": "",
                "HintIconName": "",
                "HintSymbolicIconName": "",
            },
            [],
        )
        mounts = dbus.Array([path_bytes(drive["mount"])], signature="ay")
        filesystem = objects[block_path]
        filesystem.AddProperties(
            FILESYSTEM,
            {"MountPoints": mounts, "Size": dbus.UInt64(drive["size"])},
        )
        filesystem.AddMethods(
            FILESYSTEM,
            [
                ("Mount", "a{sv}", "s", f'ret = "{drive["mount"]}"'),
                ("Unmount", "a{sv}", "", ""),
            ],
        )
