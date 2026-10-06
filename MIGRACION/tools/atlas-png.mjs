// Lossless packaging of the scientific PPM emitted by the Rust demo, without external packages.
import { readFileSync,writeFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
const input=new URL('../out/h0-atlas.ppm',import.meta.url);
const data=readFileSync(input),header=data.subarray(0,64).toString().match(/^P6\n(\d+) (\d+)\n255\n/);
if(!header)throw new Error('Expected P6 pixmap');
const w=Number(header[1]),h=Number(header[2]),rgb=data.subarray(header[0].length);
if(rgb.length!==w*h*3)throw new Error('Incomplete pixels');
function crc32(bytes){let c=0xffffffff;for(const b of bytes){c^=b;for(let i=0;i<8;i++)c=(c>>>1)^((c&1)?0xedb88320:0);}return (c^0xffffffff)>>>0;}
function chunk(kind,bytes){const k=Buffer.from(kind),length=Buffer.alloc(4),crc=Buffer.alloc(4);length.writeUInt32BE(bytes.length);crc.writeUInt32BE(crc32(Buffer.concat([k,bytes])));return Buffer.concat([length,k,bytes,crc]);}
const ihdr=Buffer.alloc(13);ihdr.writeUInt32BE(w,0);ihdr.writeUInt32BE(h,4);ihdr[8]=8;ihdr[9]=2;
const scan=Buffer.alloc((w*3+1)*h);for(let y=0;y<h;y++)rgb.copy(scan,y*(w*3+1)+1,y*w*3,(y+1)*w*3);
writeFileSync(new URL('../evidence/h0-atlas.png',import.meta.url),Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',ihdr),chunk('IDAT',deflateSync(scan)),chunk('IEND',Buffer.alloc(0))]));
console.log('evidence/h0-atlas.png');
