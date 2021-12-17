const anchor = require('@project-serum/anchor');
const assert = require('assert');
const {
  TOKEN_PROGRAM_ID,
  sleep,
  getTokenAccount,
  createMint,
  createTokenAccount,
  mintToAccount,
} = require('./utils');

let program = anchor.workspace.IdoPool;

//Read the provider from the configured environmnet.
//represents an outside actor
//owns mints out of any other actors control, provides initial $$ to others
const envProvider = anchor.Provider.local();

//we allow this convenience var to change between default env and mock user(s)
//initially we are the outside actor
let provider = envProvider;
//convenience method to set in anchor AND above convenience var
//setting in anchor allows the rpc and accounts namespaces access
//to a different wallet from env
function setProvider(p) {
  provider = p;
  anchor.setProvider(p);
  program = new anchor.Program(program.idl, program.programId, p);
}
setProvider(provider);

describe('ido-pool', () => {
  // All mints default to 6 decimal places.
  const watermelonIdoAmount = new anchor.BN(5000000);
  const maxUsdcAmount = new anchor.BN(40000000); // 40 USDC

  // These are all of the variables we assume exist in the world already and
  // are available to the client.
  let usdcMint = null;
  let watermelonMint = null;
  let creatorUsdc = null; //token account
  let creatorWatermelon = null; //token account

  it('Initializes the state-of-the-world', async () => {
    usdcMint = await createMint(provider);
    watermelonMint = await createMint(provider);
    creatorUsdc = await createTokenAccount(
      provider,
      usdcMint,
      provider.wallet.publicKey
    );
    creatorWatermelon = await createTokenAccount(
      provider,
      watermelonMint,
      provider.wallet.publicKey
    );
    // Mint Watermelon tokens the will be distributed from the IDO pool.
    await mintToAccount(
      provider,
      watermelonMint,
      creatorWatermelon,
      watermelonIdoAmount,
      provider.wallet.publicKey
    );
    creator_watermelon_account = await getTokenAccount(
      provider,
      creatorWatermelon
    );
    assert.ok(creator_watermelon_account.amount.eq(watermelonIdoAmount));
  });

  // These are all variables the client will have to create to initialize the
  // IDO pool
  let poolSigner = null; //pda of(watermelon mint)
  let redeemableMint = null; //owner: poolSigner
  let poolWatermelon = null; //owner: poolSigner
  let poolUsdc = null; //owner: poolSigner
  let poolAccount = null; //generated keypair

  let startIdoTs = null;
  let endDepositsTs = null;
  let endIdoTs = null;

  it('Initializes the IDO pool', async () => {
    // We use the watermelon mint address as the seed, could use something else though.
    const [_poolSigner, nonce] = await anchor.web3.PublicKey.findProgramAddress(
      [watermelonMint.toBuffer()],
      program.programId
    );
    poolSigner = _poolSigner;

    // Pool doesn't need a Redeemable SPL token account because it only
    // burns and mints redeemable tokens, it never stores them.
    redeemableMint = await createMint(provider, poolSigner);
    poolWatermelon = await createTokenAccount(
      provider,
      watermelonMint,
      poolSigner
    );
    poolUsdc = await createTokenAccount(provider, usdcMint, poolSigner);

    poolAccount = anchor.web3.Keypair.generate();
    const nowBn = new anchor.BN(Date.now() / 1000);
    startIdoTs = nowBn.add(new anchor.BN(5));
    endDepositsTs = nowBn.add(new anchor.BN(10));
    endIdoTs = nowBn.add(new anchor.BN(15));
    withdrawTs = nowBn.add(new anchor.BN(19));

    // Atomically create the new account and initialize it with the program.
    await program.rpc.initializePool(
      watermelonIdoAmount,
      maxUsdcAmount,
      nonce,
      startIdoTs,
      endIdoTs,
      withdrawTs,
      {
        accounts: {
          poolAccount: poolAccount.publicKey,
          poolSigner,
          distributionAuthority: provider.wallet.publicKey,
          payer: provider.wallet.publicKey,
          creatorWatermelon,
          redeemableMint,
          usdcMint,
          watermelonMint,
          poolWatermelon,
          poolUsdc,
          systemProgram: anchor.web3.SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        },
        instructions: [
          await program.account.poolAccount.createInstruction(poolAccount),
        ],
        signers: [poolAccount],
      }
    );

    creators_watermelon_account = await getTokenAccount(
      provider,
      creatorWatermelon
    );
    assert.ok(creators_watermelon_account.amount.eq(new anchor.BN(0)));
  });

  // We're going to need to start using the associated program account for creating token accounts
  // if not in testing, then definitely in production.

  let userUsdc = null; //token account
  let userRedeemable = null; //token account
  // 10 usdc
  const firstDeposit = new anchor.BN(10_000_349);

  it('Exchanges user USDC for redeemable tokens', async () => {
    // Wait until the IDO has opened.
    if (Date.now() < startIdoTs.toNumber() * 1000) {
      await sleep(startIdoTs.toNumber() * 1000 - Date.now() + 1000);
    }

    userUsdc = await createTokenAccount(
      provider,
      usdcMint,
      provider.wallet.publicKey
    );
    await mintToAccount(
      provider,
      usdcMint,
      userUsdc,
      firstDeposit,
      provider.wallet.publicKey
    );
    userRedeemable = await createTokenAccount(
      provider,
      redeemableMint,
      provider.wallet.publicKey
    );

    try {
      const tx = await program.rpc.exchangeUsdcForRedeemable(firstDeposit, {
        accounts: {
          poolAccount: poolAccount.publicKey,
          poolSigner,
          redeemableMint,
          poolUsdc,
          userAuthority: provider.wallet.publicKey,
          userUsdc,
          userRedeemable,
          tokenProgram: TOKEN_PROGRAM_ID,
          clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
        },
      });
    } catch (err) {
      console.log('This is the error message', err.toString());
    }
    poolUsdcAccount = await getTokenAccount(provider, poolUsdc);
    assert.ok(poolUsdcAccount.amount.eq(firstDeposit));
    userRedeemableAccount = await getTokenAccount(provider, userRedeemable);
    assert.ok(userRedeemableAccount.amount.eq(firstDeposit));

    let _poolAccount = await program.account.poolAccount.fetch(
      poolAccount.publicKey
    );
    assert.ok(_poolAccount.numUsdcTokens.eq(firstDeposit));
  });

  // 23 usdc
  const secondDeposit = new anchor.BN(23_000_672);
  let totalPoolUsdc = null;

  it('Exchanges a second users USDC for redeemable tokens', async () => {
    secondUserUsdc = await createTokenAccount(
      provider,
      usdcMint,
      provider.wallet.publicKey
    );
    await mintToAccount(
      provider,
      usdcMint,
      secondUserUsdc,
      secondDeposit,
      provider.wallet.publicKey
    );
    secondUserRedeemable = await createTokenAccount(
      provider,
      redeemableMint,
      provider.wallet.publicKey
    );

    await program.rpc.exchangeUsdcForRedeemable(secondDeposit, {
      accounts: {
        poolAccount: poolAccount.publicKey,
        poolSigner,
        redeemableMint,
        poolUsdc,
        userAuthority: provider.wallet.publicKey,
        userUsdc: secondUserUsdc,
        userRedeemable: secondUserRedeemable,
        tokenProgram: TOKEN_PROGRAM_ID,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      },
    });

    totalPoolUsdc = firstDeposit.add(secondDeposit);
    poolUsdcAccount = await getTokenAccount(provider, poolUsdc);
    assert.ok(poolUsdcAccount.amount.eq(totalPoolUsdc));
    secondUserRedeemableAccount = await getTokenAccount(
      provider,
      secondUserRedeemable
    );
    assert.ok(secondUserRedeemableAccount.amount.eq(secondDeposit));

    let _poolAccount = await program.account.poolAccount.fetch(
      poolAccount.publicKey
    );
    assert.ok(_poolAccount.numUsdcTokens.eq(totalPoolUsdc));
  });

  // 2 usdc
  const firstWithdrawal = new anchor.BN(2_000_000);

  it('Exchanges user Redeemable tokens for USDC', async () => {
    await program.rpc.exchangeRedeemableForUsdc(firstWithdrawal, {
      accounts: {
        poolAccount: poolAccount.publicKey,
        poolSigner,
        redeemableMint,
        poolUsdc,
        userAuthority: provider.wallet.publicKey,
        userUsdc,
        userRedeemable,
        tokenProgram: TOKEN_PROGRAM_ID,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      },
    });

    totalPoolUsdc = totalPoolUsdc.sub(firstWithdrawal);
    poolUsdcAccount = await getTokenAccount(provider, poolUsdc);
    assert.ok(poolUsdcAccount.amount.eq(totalPoolUsdc));
    userUsdcAccount = await getTokenAccount(provider, userUsdc);
    assert.ok(userUsdcAccount.amount.eq(firstWithdrawal));

    let _poolAccount = await program.account.poolAccount.fetch(
      poolAccount.publicKey
    );
    assert.ok(_poolAccount.numUsdcTokens.eq(totalPoolUsdc));
  });

  // 23 usdc (real: 9 usdc because of the max_usdc)
  const thirdDeposit = new anchor.BN(23_000_672);

  it('Exchanges a third users USDC for redeemable tokens', async () => {
    thirdUserUsdc = await createTokenAccount(
      provider,
      usdcMint,
      provider.wallet.publicKey
    );
    await mintToAccount(
      provider,
      usdcMint,
      thirdUserUsdc,
      thirdDeposit,
      provider.wallet.publicKey
    );
    thirdUserRedeemable = await createTokenAccount(
      provider,
      redeemableMint,
      provider.wallet.publicKey
    );

    await program.rpc.exchangeUsdcForRedeemable(thirdDeposit, {
      accounts: {
        poolAccount: poolAccount.publicKey,
        poolSigner,
        redeemableMint,
        poolUsdc,
        userAuthority: provider.wallet.publicKey,
        userUsdc: thirdUserUsdc,
        userRedeemable: thirdUserRedeemable,
        tokenProgram: TOKEN_PROGRAM_ID,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      },
    });

    thirdUserRedeemableAccount = await getTokenAccount(
      provider,
      thirdUserRedeemable
    );
    assert.ok(
      thirdUserRedeemableAccount.amount.eq(maxUsdcAmount.sub(totalPoolUsdc))
    );

    totalPoolUsdc = maxUsdcAmount;
    poolUsdcAccount = await getTokenAccount(provider, poolUsdc);
    assert.ok(poolUsdcAccount.amount.eq(totalPoolUsdc));

    let _poolAccount = await program.account.poolAccount.fetch(
      poolAccount.publicKey
    );
    assert.ok(_poolAccount.numUsdcTokens.eq(totalPoolUsdc));
  });

  let remainingWatermelon = null;

  it('Exchanges user Redeemable tokens for watermelon', async () => {
    // Wait until the IDO has opened.
    if (Date.now() < withdrawTs.toNumber() * 1000) {
      await sleep(withdrawTs.toNumber() * 1000 - Date.now() + 2000);
    }
    let firstUserRedeemable = firstDeposit.sub(firstWithdrawal);
    userWatermelon = await createTokenAccount(
      provider,
      watermelonMint,
      provider.wallet.publicKey
    );

    await program.rpc.exchangeRedeemableForWatermelon(firstUserRedeemable, {
      accounts: {
        poolAccount: poolAccount.publicKey,
        poolSigner,
        redeemableMint,
        poolWatermelon,
        userAuthority: provider.wallet.publicKey,
        userWatermelon,
        userRedeemable,
        tokenProgram: TOKEN_PROGRAM_ID,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      },
    });

    poolWatermelonAccount = await getTokenAccount(provider, poolWatermelon);
    let redeemedWatermelon = firstUserRedeemable
      .mul(watermelonIdoAmount)
      .div(totalPoolUsdc);
    remainingWatermelon = watermelonIdoAmount.sub(redeemedWatermelon);
    assert.ok(poolWatermelonAccount.amount.eq(remainingWatermelon));
    userWatermelonAccount = await getTokenAccount(provider, userWatermelon);
    assert.ok(userWatermelonAccount.amount.eq(redeemedWatermelon));

    let _poolAccount = await program.account.poolAccount.fetch(
      poolAccount.publicKey
    );
    assert.ok(_poolAccount.numUsdcTokens.eq(totalPoolUsdc));
  });

  it('Exchanges second users Redeemable tokens for watermelon', async () => {
    secondUserWatermelon = await createTokenAccount(
      provider,
      watermelonMint,
      provider.wallet.publicKey
    );

    await program.rpc.exchangeRedeemableForWatermelon(secondDeposit, {
      accounts: {
        poolAccount: poolAccount.publicKey,
        poolSigner,
        redeemableMint,
        poolWatermelon,
        userAuthority: provider.wallet.publicKey,
        userWatermelon: secondUserWatermelon,
        userRedeemable: secondUserRedeemable,
        tokenProgram: TOKEN_PROGRAM_ID,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      },
    });

    poolWatermelonAccount = await getTokenAccount(provider, poolWatermelon);
    let redeemedWatermelon = secondDeposit
      .mul(watermelonIdoAmount)
      .div(totalPoolUsdc);
    remainingWatermelon = remainingWatermelon.sub(redeemedWatermelon);
    assert.ok(poolWatermelonAccount.amount.eq(remainingWatermelon));
    userWatermelonAccount = await getTokenAccount(
      provider,
      secondUserWatermelon
    );
    assert.ok(userWatermelonAccount.amount.eq(redeemedWatermelon));
  });

  it('Withdraws total USDC from pool account', async () => {
    const acc = await getTokenAccount(provider, poolUsdc);
    await program.rpc.withdrawPoolUsdc(new anchor.BN(acc.amount), {
      accounts: {
        poolAccount: poolAccount.publicKey,
        poolSigner,
        distributionAuthority: provider.wallet.publicKey,
        creatorUsdc,
        poolUsdc,
        payer: provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      },
    });

    poolUsdcAccount = await getTokenAccount(provider, poolUsdc);
    assert.ok(poolUsdcAccount.amount.eq(new anchor.BN(0)));
    creatorUsdcAccount = await getTokenAccount(provider, creatorUsdc);
    assert.ok(creatorUsdcAccount.amount.eq(totalPoolUsdc));
  });

  it('Modify ido time', async () => {
    await program.rpc.modifyIdoTime(
      new anchor.BN(1),
      new anchor.BN(2),
      new anchor.BN(3),
      {
        accounts: {
          poolAccount: poolAccount.publicKey,
          distributionAuthority: provider.wallet.publicKey,
          payer: provider.wallet.publicKey,
        },
      }
    );
    const pool = await program.account.poolAccount.fetch(poolAccount.publicKey);
    assert.equal(pool.startIdoTs.toString(), '1');
    assert.equal(pool.endIdoTs.toString(), '2');
    assert.equal(pool.withdrawMelonTs.toString(), '3');
  });

  it('Modify max usdc tokens', async () => {
    await program.rpc.modifyMaxUsdcTokens(new anchor.BN(100000000), {
      accounts: {
        poolAccount: poolAccount.publicKey,
        distributionAuthority: provider.wallet.publicKey,
        payer: provider.wallet.publicKey,
      },
    });
    const pool = await program.account.poolAccount.fetch(poolAccount.publicKey);
    assert.equal(pool.maxUsdcTokens.toString(), '100000000');
  });
});
