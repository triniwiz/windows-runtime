console.log("Hello From NativeScript running in a Windows CLI App\n");
console.log(performance.now() + '\n');
console.dir(global + '\n');
//console.dir(Windows.UI.Popups.Placement);
console.log('Default', Windows.UI.Popups.Placement.Default, Windows.UI.Popups.Placement.Default === 0);
console.log('Right', Windows.UI.Popups.Placement.Right, Windows.UI.Popups.Placement.Right === 4);
console.log("\n")


