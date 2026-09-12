// Source-faithful pixel partition: original pixels are assigned to named parts.
// Regenerated expression art is only used inside explicitly traced face patches.
const fs = require('node:fs');
const path = require('node:path');
const sharp = require('sharp');
const { createCanvas, Path2D } = require('@napi-rs/canvas');
const { initializeCanvas, writePsdBuffer, readPsd } = require('ag-psd');
initializeCanvas(createCanvas);
const ROOT = path.resolve(__dirname, '..');
const OUT = path.join(ROOT, 'output');
const PARTS = path.join(OUT, 'parts');
fs.mkdirSync(PARTS, { recursive: true });
const defs = [
  ['BodyCore', '身体_黑色上衣与腰部', 'body', 'M0 0H1225V1284H0Z'],
  ['CoatRearL', '后外套_画面左', 'body', 'M398 574 Q438 571 470 609 L442 745 L354 852 L279 899 L207 875 L171 810 L280 725 L309 659Z'],
  ['CoatRearR', '后外套_画面右', 'body', 'M760 575 Q855 559 916 624 L978 732 L996 847 L956 923 L848 864 L790 716Z'],
  ['RibbonLongL', '长飘带_画面左', 'ribbon', 'M315 737 L356 767 L269 828 Q209 892 178 938 L141 971 L32 958 L83 901 L193 827Z'],
  ['RibbonLongR', '长飘带_画面右', 'ribbon', 'M930 743 L991 788 L1125 865 L1199 919 L1093 954 L1038 886 L971 844Z'],
  ['RibbonLowerL', '垂落飘带_画面左', 'ribbon', 'M387 916 L445 944 L482 1104 L381 1067 L390 1013Z'],
  ['Hips', '短裤与坐姿下摆', 'body', 'M586 716 Q679 710 754 747 L849 939 L820 976 L730 1005 L587 1003 L492 982 L454 946 L493 806Z'],
  ['LegBack', '后侧腿_画面右', 'legs', 'M745 764 Q788 749 833 777 Q858 789 879 830 L967 1018 L889 1068 Q841 1000 807 953 L755 928 L721 849Z'],
  ['LegFront', '前侧交叉腿', 'legs', 'M714 714 Q775 704 804 738 Q829 766 817 805 L750 956 L739 1006 L614 1009 L624 947 Q566 972 528 945 L484 931 L495 882 L565 811Z'],
  ['BootBack', '运动靴_画面右', 'boots', 'M861 1007 Q893 975 941 981 L997 996 L1022 1090 Q1084 1141 1089 1229 L1039 1263 L911 1250 L844 1232 L832 1191 L833 1119Z'],
  ['BootFront', '运动靴_画面左', 'boots', 'M596 987 Q619 963 676 962 L743 978 L763 1023 L761 1070 Q797 1091 814 1159 L819 1210 L784 1243 L666 1249 L572 1243 L551 1212 L553 1153 L576 1076 L568 1048Z'],
  ['SleeveL', '宽袖_画面左', 'arms', 'M484 575 Q543 574 575 627 L583 679 L559 745 L477 874 L445 968 L399 1009 L316 984 L268 960 L274 903 L302 860 L363 800 L427 679Z'],
  ['SleeveR', '宽袖_画面右', 'arms', 'M818 695 L896 695 L946 753 Q1001 811 1019 856 L1012 899 L971 960 L934 980 L909 954 L852 847 L812 788Z'],
  ['HandL', '手掌_画面左', 'hands', 'M305 956 Q332 944 362 958 L389 975 L394 1000 L369 1043 L352 1048 L337 1021 L297 1034 L269 1024 L237 1009 L247 991Z'],
  ['HandR', '手掌_画面右', 'hands', 'M1020 925 Q1054 927 1081 945 L1125 969 L1137 985 L1118 998 L1091 993 L1074 1010 L1056 1001 L1038 981 L1006 976 L984 960Z'],
  ['ShoulderL', '肩膀_画面左', 'body', 'M510 546 Q527 511 555 512 Q583 510 592 542 L598 569 L569 588Z'],
  ['ShoulderR', '肩膀_画面右', 'body', 'M707 551 Q752 545 775 569 Q792 599 789 647 L753 685 L743 613Z'],
  ['Collar', '立领', 'body', 'M578 519 L639 504 L689 511 L697 551 L714 573 L644 583 L598 558Z'],
  ['HairBack', '后发_卷曲底层', 'head', 'M210 0H1020V440 L893 533 L785 568 L712 551 L687 515 L602 515 L591 544 L557 511 L539 518 L512 541 L487 576 L374 564 L266 529 L211 403Z'],
  ['Bow', '头顶蝴蝶结', 'head', 'M574 58 L603 15 L630 23 L653 47 L686 29 L699 50 L736 2 L809 0 L851 32 L893 103 L956 170 L975 236 L1003 270 L973 322 L990 418 L950 374 L932 348 L882 289 L851 206 L785 163 L719 111 L674 79Z'],
  ['Face', '脸部', 'head', 'M584 206 Q606 225 635 263 L694 309 L738 342 L761 341 L767 389 L748 425 Q735 465 696 488 Q662 512 611 518 Q562 530 521 509 L496 496 L478 479 L455 454 L442 410 L459 365 L501 330 L546 280Z'],
  ['Ear', '耳朵与耳饰', 'head', 'M753 351 Q772 333 791 342 L817 365 L816 401 L799 423 L787 452 L765 446 L750 423Z'],
  ['HairSideL', '侧发_画面左', 'head', 'M337 207 Q395 175 433 237 L453 321 L445 374 Q416 433 466 481 L504 493 Q479 507 453 493 Q475 515 515 518 L501 554 L449 560 L385 536 L315 533 L287 494 L276 436 L294 354Z'],
  ['HairSideR', '侧发_画面右', 'head', 'M719 266 L780 291 Q839 308 867 351 L861 453 L893 477 L850 520 L794 546 L729 564 L691 548 L717 493 Q742 448 736 421 Q756 377 733 340Z'],
  ['FringeLeft', '刘海_画面左大片', 'head', 'M282 277 Q304 174 368 110 Q433 58 511 76 L542 123 Q483 169 472 252 Q459 330 431 371 Q406 404 366 412 L340 444 L292 443 Q322 408 322 373 Q282 399 271 352 Q247 325 282 277Z'],
  ['FringeCenterL', '刘海_中左发束', 'head', 'M483 121 Q510 100 546 121 L540 158 Q492 196 491 264 Q494 328 540 369 L556 388 Q518 393 487 366 Q469 345 465 316 Q447 346 432 353 Q425 280 447 213Z'],
  ['FringeCenter', '刘海_中间发束', 'head', 'M548 116 Q571 114 592 136 L607 207 Q582 230 588 290 L612 351 Q560 345 527 316 Q495 289 493 250 Q482 180 520 139Z'],
  ['FringeRight', '刘海_画面右大片', 'head', 'M580 92 Q628 57 676 78 Q736 88 779 142 Q799 174 817 223 L845 251 L893 255 Q858 279 822 266 L800 281 Q746 267 706 216 L646 164 Q670 223 684 270 L694 314 Q676 301 667 285 Q643 262 622 231 L598 187 L581 149Z'],
  ['HairClipLime', '荧光黄发卡', 'accessories', 'M688 207 L757 176 L782 213 L771 226 L782 236 L799 250 L768 276 L743 261 L744 248 L706 247Z'],
  ['HairClipCross', '蓝色十字发卡', 'accessories', 'M678 270 L711 276 L720 257 L739 266 L729 289 L752 300 L745 318 L720 307 L708 326 L689 318 L700 298 L678 285Z'],
  ['EyeL', '眼睛_画面左_睫毛眼白', 'face', 'M439 437 L451 417 L454 399 L463 398 Q490 386 516 390 Q541 394 555 419 L555 449 Q528 471 500 472 L477 460 L459 447Z'],
  ['EyeR', '眼睛_画面右_睫毛眼白', 'face', 'M609 373 Q613 345 638 334 L649 321 L653 329 L671 316 L679 326 L705 328 L721 320 L717 335 L735 341 L718 353 L715 384 L693 408 Q668 420 643 405 L624 392Z'],
  ['IrisL', '虹膜_画面左', 'face', 'M489 406 Q508 394 529 406 Q550 417 549 443 L532 458 Q510 465 496 449 Q481 430 489 406Z'],
  ['IrisR', '虹膜_画面右', 'face', 'M640 353 Q659 340 678 347 Q700 360 697 386 Q695 405 674 408 Q650 410 640 392 Q632 374 640 353Z'],
  ['BrowL', '眉毛_画面左', 'face', 'M459 344 Q480 338 504 339 L505 347 Q480 345 459 353Z'],
  ['BrowR', '眉毛_画面右', 'face', 'M602 305 Q628 286 658 287 L663 295 Q633 293 607 312Z'],
  ['Mouth', '张嘴微笑', 'face', 'M578 471 Q604 458 627 449 Q641 449 643 465 Q644 488 623 497 Q596 501 582 484Z'],
];
// Foreground legs occlude the jacket, and the exposed ear occludes side hair.
for (const id of ['LegBack','LegFront','BootBack','BootFront']) {
 const entry=defs.splice(defs.findIndex(d=>d[0]===id),1)[0];
 defs.splice(defs.findIndex(d=>d[0]==='HandL'),0,entry);
}
{ const entry=defs.splice(defs.findIndex(d=>d[0]==='Ear'),1)[0];defs.splice(defs.findIndex(d=>d[0]==='EyeL'),0,entry); }

function maskFor(d,w,h) { const c=createCanvas(w,h); const ctx=c.getContext('2d');ctx.fillStyle='#fff';ctx.fill(new Path2D(d));return ctx.getImageData(0,0,w,h).data; }
function bbox(data,w,h){let x0=w,y0=h,x1=-1,y1=-1;for(let y=0;y<h;y++)for(let x=0;x<w;x++)if(data[(y*w+x)*4+3]){x0=Math.min(x0,x);y0=Math.min(y0,y);x1=Math.max(x1,x);y1=Math.max(y1,y);}return x1<0?null:{left:x0,top:y0,width:x1-x0+1,height:y1-y0+1};}
function composite(dst,src) {for(let i=0;i<dst.length;i+=4){let a=src[i+3]/255,b=dst[i+3]/255,o=a+b*(1-a);if(!o)continue;for(let c=0;c<3;c++)dst[i+c]=Math.round((src[i+c]*a+dst[i+c]*b*(1-a))/o);dst[i+3]=Math.round(o*255);} }

(async()=>{
 const sourceFile=path.join(ROOT,'assets/reference/molly-original.png');
 const {data:src,info}=await sharp(sourceFile).ensureAlpha().raw().toBuffer({resolveWithObject:true});
 const {width:w,height:h}=info,n=w*h;
 const owner=new Uint16Array(n);
 for(let k=0;k<defs.length;k++){const m=maskFor(defs[k][3],w,h);for(let i=0;i<n;i++)if(m[i*4+3]>=128&&src[i*4+3])owner[i]=k;}
 // Reassign small distant remainder fragments to their nearest actual part.
 // This keeps the torso's ArtMesh bounds local instead of spanning stray edge pixels.
 const queue=new Int32Array(n),seen=new Uint8Array(n);let tail=0;
 for(let i=0;i<n;i++)if(src[i*4+3]&&owner[i]!==0){queue[tail++]=i;seen[i]=1;}
 const nearest=owner.slice();
 for(let head=0;head<tail;head++){const i=queue[head],x=i%w;for(const j of [x?i-1:-1,x<w-1?i+1:-1,i-w,i+w])if(j>=0&&j<n&&!seen[j]){seen[j]=1;nearest[j]=nearest[i];queue[tail++]=j;}}
 for(let i=0;i<n;i++){const x=i%w,y=Math.floor(i/w);if(owner[i]===0&&(y<530||y>978||x<429||x>818))owner[i]=nearest[i];}
 const fulls=defs.map(()=>Buffer.alloc(n*4));
 for(let i=0;i<n;i++) if(src[i*4+3]) src.copy(fulls[owner[i]],i*4,i*4,i*4+4);
 const layers=[],manifest=[];
 async function add(id,name,group,raw,hidden=false,d='') {
  const box=bbox(raw,w,h); if(!box)return;
  const cropped=await sharp(raw,{raw:{width:w,height:h,channels:4}}).extract(box).raw().toBuffer();
  const filename=id+'.png';
  await sharp(cropped,{raw:{width:box.width,height:box.height,channels:4}}).png().toFile(path.join(PARTS,filename));
  // PSD opacity remains 1 so Cubism imports the expression sprites. The UI is reset by MCP after import.
  layers.push({name:id+'__'+name,top:box.top,left:box.left,imageData:{width:box.width,height:box.height,data:new Uint8ClampedArray(cropped)},opacity:hidden?0:1});
  manifest.push({id,name,group,file:'parts/'+filename,...box,defaultOpacity:hidden?0:100,path:d});
 }
 for(let k=0;k<defs.length;k++)await add(...defs[k].slice(0,3),fulls[k],false,defs[k][3]);
 const composed=Buffer.alloc(n*4);for(const f of fulls)composite(composed,f);
 let bad=0,maxDiff=0;for(let i=0;i<n;i++){if(!src[i*4+3])continue;for(let c=0;c<4;c++){const d=Math.abs(src[i*4+c]-composed[i*4+c]);if(d)bad++;maxDiff=Math.max(maxDiff,d);}}
 await sharp(composed,{raw:{width:w,height:h,channels:4}}).png().toFile(path.join(OUT,'molly-default-composite.png'));
 const generated=await sharp(path.join(ROOT,'assets/generated/molly-closed-expression.png')).resize(w,h,{fit:'fill'}).ensureAlpha().raw().toBuffer();
 // Broad expression patches cover the original eyelid/iris extent, while their periphery stays within skin.
 const patches=[
  ['EyeClosedL','闭眼_画面左','face','M438 438 L451 416 L454 397 L464 394 Q503 379 539 399 Q559 417 559 450 Q537 475 503 475 L477 461 L459 449Z'],
  ['EyeClosedR','闭眼_画面右','face','M608 374 Q610 346 636 332 L649 319 L654 327 L672 314 L680 325 L706 326 L723 319 L719 335 L736 340 L721 355 L718 386 L695 411 Q668 423 641 407 L622 394Z'],
  ['MouthClosed','闭嘴微笑','face','M573 468 Q601 453 626 446 Q644 443 648 462 Q651 487 628 501 Q597 507 580 489Z']
 ];
 for(const [id,name,group,d]of patches){const m=maskFor(d,w,h),raw=Buffer.alloc(n*4);for(let i=0;i<n;i++)if(m[i*4+3]&&src[i*4+3]){for(let c=0;c<3;c++)raw[i*4+c]=generated[i*4+c];raw[i*4+3]=Math.min(src[i*4+3],m[i*4+3]);}await add(id,name,group,raw,true,d);}
 const psd={width:w,height:h,channels:4,colorMode:3,bitsPerChannel:8,imageData:{width:w,height:h,data:new Uint8ClampedArray(composed)},children:layers};
 fs.writeFileSync(path.join(OUT,'Molly_Live2D_Layers.psd'),writePsdBuffer(psd,{generateThumbnail:false}));
 const check=readPsd(fs.readFileSync(path.join(OUT,'Molly_Live2D_Layers.psd')),{skipLayerImageData:true,skipCompositeImageData:true,skipThumbnail:true});
 fs.writeFileSync(path.join(OUT,'parts-manifest.json'),JSON.stringify({width:w,height:h,source:'../assets/reference/molly-original.png',expressionSource:'../assets/generated/molly-closed-expression.png',layers:manifest,validation:{changedChannelsOnVisiblePixels:bad,maxChannelDifference:maxDiff,psdLayersReadBack:check.children.length}},null,2));
 // A labelled contact sheet makes the exact decomposition reviewable.
 const cw=1100,ch=Math.ceil(manifest.length/5)*210,contact=createCanvas(cw,ch),cx=contact.getContext('2d');cx.fillStyle='#e7e9ef';cx.fillRect(0,0,cw,ch);
 const {loadImage}=require('@napi-rs/canvas');
 for(let k=0;k<manifest.length;k++){const m=manifest[k],x=(k%5)*220,y=Math.floor(k/5)*210;cx.fillStyle='#f8f8fb';cx.fillRect(x+5,y+5,210,200);const im=await loadImage(path.join(OUT,m.file));const sc=Math.min(194/im.width,164/im.height);cx.drawImage(im,x+110-im.width*sc/2,y+9+(164-im.height*sc)/2,im.width*sc,im.height*sc);cx.fillStyle='#202638';cx.font='12px sans-serif';cx.fillText(m.id,x+12,y+188);}
 fs.writeFileSync(path.join(OUT,'parts-contact-sheet.png'),contact.toBuffer('image/png'));
 console.log(JSON.stringify({width:w,height:h,layers:manifest.length,changedChannelsOnVisiblePixels:bad,maxChannelDifference:maxDiff,psdLayersReadBack:check.children.length,psd:'output/Molly_Live2D_Layers.psd'},null,2));
})().catch(e=>{console.error(e);process.exitCode=1;});
