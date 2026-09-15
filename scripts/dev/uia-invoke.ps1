param(
    [Parameter(Mandatory)][string]$Window,   # top-level window name (exact or wildcard)
    [string]$Invoke,                         # button name to press
    [switch]$List                            # list button names under the window
)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::RootElement
$wins = $root.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition) |
    Where-Object { $_.Current.Name -like $Window }
if (-not $wins) { Write-Output "no window matching '$Window'"; exit 2 }
foreach ($w in $wins) {
    $btnCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $buttons = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, $btnCond)
    if ($List) { $buttons | ForEach-Object { "[$($w.Current.Name)] button: $($_.Current.Name)" } }
    if ($Invoke) {
        $b = $buttons | Where-Object { $_.Current.Name -eq $Invoke } | Select-Object -First 1
        if ($b) {
            $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
            Write-Output "invoked '$Invoke' in '$($w.Current.Name)'"
            exit 0
        }
    }
}
if ($Invoke) { Write-Output "button '$Invoke' not found"; exit 3 }
