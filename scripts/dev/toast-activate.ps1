param(
    [string]$Clsid = 'D0015A10-DE2B-4104-8D25-A33092340D48',
    [string]$Aumid = 'app.dun.dev',
    [Parameter(Mandatory)][string]$Arguments
)
# Calls Dun's toast activator the same way the Windows shell does: create the
# registered COM class (starting the LocalServer32 EXE if needed) and invoke
# INotificationActivationCallback::Activate with a button's argument string.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

[ComImport, Guid("53E31837-6600-4A81-9395-75CFFE746F94"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface INotificationActivationCallback {
    void Activate([MarshalAs(UnmanagedType.LPWStr)] string appUserModelId,
                  [MarshalAs(UnmanagedType.LPWStr)] string invokedArgs,
                  IntPtr data, uint count);
}

public static class DunToastActivate {
    public static void Run(Guid clsid, string aumid, string args) {
        Type t = Type.GetTypeFromCLSID(clsid, true);
        object o = Activator.CreateInstance(t);
        try { ((INotificationActivationCallback)o).Activate(aumid, args, IntPtr.Zero, 0); }
        finally { Marshal.ReleaseComObject(o); }
    }
}
'@
$sw = [Diagnostics.Stopwatch]::StartNew()
[DunToastActivate]::Run([guid]$Clsid, $Aumid, $Arguments)
"activated in $($sw.ElapsedMilliseconds) ms"
