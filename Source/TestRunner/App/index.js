console.time('WindowsRuntime');
console.log('Hi Osei');
console.log('time', __time());
try{
    console.log('Windows', Windows);
}catch(e){
    console.log(e);
}
console.timeEnd('WindowsRuntime');

