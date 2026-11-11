// Well-known modern skins (.wal), first in the Skin Museum's Modern tab. The
// museum lists its modern skins in no particular order and has no picture of
// most of these, so each has a small one here, taken in Spotiamp+ (every one
// of them was worn and played with there first).
import winampModern from "./featured/winamp-modern.webp";
import bigBento from "./featured/big-bento.webp";
import bento from "./featured/bento.webp";
import classicModern from "./featured/classic-modern.webp";
import mmd3 from "./featured/mmd3.webp";
import cornerAmp from "./featured/corneramp-redux.webp";
import doublehelix from "./featured/doublehelix.webp";
import arion from "./featured/arion.webp";
import drone from "./featured/drone.webp";
import gc from "./featured/gc.webp";
import sideAmp from "./featured/side-amp.webp";

/** @type {{name: string, md5: string, shot: string}[]} */
export const FEATURED = [
  ["Winamp Modern", "f2c1b80ef4f71d76f34915463793bd5e", winampModern],
  ["Big Bento", "1728b11b37e40cf359b70fc0903dcc3e", bigBento],
  ["Bento", "d84a2ac19b8115d32b17a7af171da98c", bento],
  ["Winamp Classic Modern v1.05 by Victhor", "91aa5a100f631f07bc99c5fdff3cebe2", classicModern],
  ["MMD3", "c9604216fb99fa7bb3f5813610b5c63c", mmd3],
  ["CornerAmp Redux", "40966514b60270c6134fb4471fc4afc7", cornerAmp],
  ["Doublehelix", "e0f07c96a405b7500873215841a10723", doublehelix],
  ["Arion", "32f0ef8ec2bdec8d13fc09d69e5db1f8", arion],
  ["Drone", "d44fe8064bd3aae4d68d76246b63621f", drone],
  ["GC", "b0e112ccd13343a973472e44e85111aa", gc],
  ["side-amp", "59317fd261fcbc0bc7221ce30b37fd50", sideAmp],
].map(([name, md5, shot]) => ({ name, md5, shot }));
