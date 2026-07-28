namespace DotNetBridgeTests;

// Test-only stand-ins for a real WinRT base class with virtuals JS may or may not override —
// mirrors why MeasureOverrideBase exists (the real target, a WinUI FrameworkElement, needs the
// full WinRT runtime the xunit host doesn't have).
public class CallBaseFallbackBase
{
    public virtual string Describe()
    {
        return "base-describe";
    }

    public virtual string Greet(string name)
    {
        return "base-greet:" + name;
    }

    public virtual string Text
    {
        get { return "base-text"; }
        set { LastSetText = value; }
    }

    public string? LastSetText;
}

// Test-only stand-in for a WinRT interface (e.g. INotifyPropertyChanged) JS implements directly
// without a real .NET base class — proves AddInterfaceImplementation + dispatch mechanically,
// independent of the CsWinRT CCW question (covered separately by real-app validation).
public interface ITestNotify
{
    string Notify(string message);
}
