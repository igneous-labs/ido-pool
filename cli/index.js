const anchor = require("@project-serum/anchor");
const serum = require("@project-serum/common");
const { TOKEN_PROGRAM_ID } = require("@solana/spl-token");
const yargs = require('yargs/yargs');
const { hideBin } = require('yargs/helpers')

process.env.ANCHOR_PROVIDER_URL = "http://localhost:8899";
process.env.ANCHOR_WALLET = `${process.env.HOME}/.config/solana/id.json`;
const provider = anchor.Provider.env();
// Configure the client to use the local cluster.
anchor.setProvider(provider);

const program = anchor.workspace.IdoPool;

async function initPool(
  usdcMint, watermelonMint, creatorWatermelon, watermelonIdoAmount, usdcMaxAmount,
  startIdoTs, endIdoTs, withdrawMelonTs) {

  // We use the watermelon mint address as the seed, could use something else though.
  const [_poolSigner, nonce] = await anchor.web3.PublicKey.findProgramAddress(
    [watermelonMint.toBuffer()],
    program.programId
  );
  poolSigner = _poolSigner;

  // fetch usdc mint to set redeemable decimals to the same value
  const mintInfo = await serum.getMintInfo(provider, usdcMint)

  // Pool doesn't need a Redeemable SPL token account because it only
  // burns and mints redeemable tokens, it never stores them.
  redeemableMint = await serum.createMint(provider, poolSigner, mintInfo.decimals);
  poolWatermelon = await serum.createTokenAccount(provider, watermelonMint, poolSigner);
  poolUsdc = await serum.createTokenAccount(provider, usdcMint, poolSigner);
  poolAccount = anchor.web3.Keypair.generate();
  distributionAuthority = provider.wallet.publicKey;


  console.log(
    'initializePool', watermelonIdoAmount.toString(),usdcMaxAmount.toString(),
    nonce, startIdoTs.toString(), endIdoTs.toString(), withdrawMelonTs.toString()
  );
  // Atomically create the new account and initialize it with the program.
  await program.rpc.initializePool(
    watermelonIdoAmount,
    usdcMaxAmount,
    nonce,
    startIdoTs,
    endIdoTs,
    withdrawMelonTs,
    {
      accounts: {
        poolAccount: poolAccount.publicKey,
        poolSigner,
        distributionAuthority,
        creatorWatermelon,
        redeemableMint,
        usdcMint,
        poolWatermelon,
        poolUsdc,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
        systemProgram: anchor.web3.SystemProgram.programId, 
      },
      signers: [poolAccount],
    }
  );

  console.log(`🏦 IDO pool initialized with ${watermelonIdoAmount.toString()} atomic tokens`);
  console.log(`Pool Account: ${poolAccount.publicKey.toBase58()}`);
  console.log(`Pool Authority: ${distributionAuthority.toBase58()}`);
  console.log(`Redeem Mint: ${redeemableMint.toBase58()}`);
  console.log(`🍉 Account: ${poolWatermelon.toBase58()}`);
  console.log(`💵 Account: ${poolUsdc.toBase58()}`);
}

const usdc_mint = {
  describe: 'the mint of the token sale bids 💵',
  type: 'string'
}

const watermelon_mint = {
  describe: 'the mint of the token for sale 🍉',
  type: 'string'
}

const start_time = {
  describe: 'the unix time at which the token sale is starting',
  default: 10 + (Date.now() / 1000),
  type: 'number'
}

const deposit_duration = {
  describe: 'the number of seconds users can deposit into the pool',
  default: 24 * 60 * 60,
  type: 'number'
}

const cancel_duration = {
  describe: 'the number of seconds users can withdraw from the pool to cancel their bid',
  default: 24 * 60 * 60,
  type: 'number'
}


yargs(hideBin(process.argv))
  .command(
    'init <usdc_mint> <watermelon_mint> <watermelon_account> <watermelon_amount>',
    'initialize IDO pool',
    y => y
      .positional('usdc_mint', usdc_mint)
      .positional('watermelon_mint', watermelon_mint)
      .positional('watermelon_account', { describe: 'the account supplying the token for sale 🍉', type: 'string' })
      .positional('watermelon_amount', { describe: 'the amount of tokens offered in this sale 🍉, in atomics', type: 'number' })
      .positional('max_usdc_amount', { decribe: 'the max amount of 💵 tokens that can be deposited before the auction ends, in atomics', type: 'number' })
      .option('start_time', start_time)
      .option('deposit_duration', deposit_duration)
      .option('cancel_duration', cancel_duration),
    args => {
      const start = new anchor.BN(args.start_time);
      const endIdo = new anchor.BN(args.deposit_duration).add(start);
      const withdrawMelon = new anchor.BN(args.cancel_duration).add(endIdo);
      initPool(
        new anchor.web3.PublicKey(args.usdc_mint),
        new anchor.web3.PublicKey(args.watermelon_mint),
        new anchor.web3.PublicKey(args.watermelon_account),
        new anchor.BN(args.watermelon_amount),
        new anchor.BN(args.max_usdc_amount),
        start,
        endIdo,
        withdrawMelon
      );
    })
  .argv;
