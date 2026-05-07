function dump(name, value) {
	const proto = Object.getPrototypeOf(value);
	const parentProto = proto ? Object.getPrototypeOf(proto) : null;
	console.log(name + " own:", Object.getOwnPropertyNames(value).sort().join(", "));
	console.log(name + " proto:", Object.getOwnPropertyNames(proto).sort().join(", "));
	console.log(name + " proto2:", parentProto ? Object.getOwnPropertyNames(parentProto).sort().join(", ") : "<none>");
	console.log(name + " children:", typeof value.Children, value.Children);
	console.log(name + " items:", typeof value.Items, value.Items);
	console.log(name + " orientation:", typeof value.Orientation, value.Orientation);
	console.log(name + " spacing:", typeof value.Spacing, value.Spacing);
	console.log(name + " probe:", typeof value.__probe__, value.__probe__);
}

dump("StackPanel", new Windows.UI.Xaml.Controls.StackPanel());
dump("ListView", new Windows.UI.Xaml.Controls.ListView());
dump("ComboBox", new Windows.UI.Xaml.Controls.ComboBox());
