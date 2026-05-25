namespace DotNetBridgeTests;

public class MeasureOverrideBase
{
    // Simple test-only virtual method that mimics a FrameworkElement-style override.
    public virtual int MeasureOverride(int available)
    {
        return 0;
    }
}
